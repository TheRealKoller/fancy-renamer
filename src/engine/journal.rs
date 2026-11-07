use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::engine::{ExecuteError, ExecutionReport};

/// So viele Läufe bleiben im Journal; ältere fallen heraus.
pub const MAX_ENTRIES: usize = 20;

/// Eine umbenannte Datei samt Zustand nach dem Umbenennen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalRename {
    pub from: PathBuf,
    pub to: PathBuf,
    pub size: u64,
    /// mtime als Sekunden seit UNIX_EPOCH.
    pub modified: u64,
    /// Nanosekundenanteil der mtime.
    pub modified_nanos: u32,
}

impl JournalRename {
    /// Prüft, ob die Zieldatei noch dem Zustand nach dem Umbenennen entspricht.
    fn target_unchanged(&self) -> bool {
        metadata_signature(&self.to) == Some((self.size, self.modified, self.modified_nanos))
    }
}

/// Ein Lauf mit allen Umbenennungen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Zeitpunkt als Sekunden seit UNIX_EPOCH.
    pub timestamp: u64,
    pub renames: Vec<JournalRename>,
}

impl JournalEntry {
    /// Baut einen Eintrag aus einem erfolgreichen Lauf; Dateien, deren Zustand
    /// nicht lesbar ist, fallen heraus.
    pub fn from_report(report: &ExecutionReport) -> Option<Self> {
        let renames = report
            .renamed
            .iter()
            .filter_map(|(from, to)| {
                let (size, modified, modified_nanos) = metadata_signature(to)?;
                Some(JournalRename {
                    from: from.clone(),
                    to: to.clone(),
                    size,
                    modified,
                    modified_nanos,
                })
            })
            .collect::<Vec<_>>();
        if renames.is_empty() {
            return None;
        }
        Some(Self {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or(0),
            renames,
        })
    }
}

/// Journal aller Läufe, ältester zuerst.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    #[serde(default)]
    pub entries: Vec<JournalEntry>,
}

impl Journal {
    /// Liest das Journal; fehlende oder defekte Dateien ergeben ein leeres
    /// Journal statt eines Fehlers.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Schreibt das Journal atomar (temporäre Datei, dann umbenennen).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, path)
    }

    /// Hängt einen Lauf an und kürzt auf [`MAX_ENTRIES`].
    pub fn push(&mut self, entry: JournalEntry) {
        self.entries.push(entry);
        let excess = self.entries.len().saturating_sub(MAX_ENTRIES);
        if excess > 0 {
            self.entries.drain(..excess);
        }
    }

    pub fn last(&self) -> Option<&JournalEntry> {
        self.entries.last()
    }

    /// Entfernt den jüngsten Lauf.
    pub fn pop(&mut self) -> Option<JournalEntry> {
        self.entries.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Eine Datei, die nicht zurückbenannt werden konnte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoSkip {
    pub path: PathBuf,
    pub reason: &'static str,
}

/// Ergebnis eines Rückgängigmachens.
#[derive(Debug, Default, Clone)]
pub struct UndoReport {
    /// (aktueller Pfad, wiederhergestellter Pfad)
    pub restored: Vec<(PathBuf, PathBuf)>,
    pub skipped: Vec<UndoSkip>,
}

impl UndoReport {
    pub fn restored_count(&self) -> usize {
        self.restored.len()
    }
}

/// Ein vorbereiteter Rückschritt.
struct Step {
    current: PathBuf,
    temporary: PathBuf,
    original: PathBuf,
}

/// Macht einen Journal-Eintrag rückgängig. Dateien, die sich seit dem
/// Umbenennen verändert haben oder deren Originalpfad wieder belegt ist,
/// werden übersprungen; alle übrigen werden zweiphasig zurückbenannt.
pub fn undo(entry: &JournalEntry) -> Result<UndoReport, ExecuteError> {
    let mut report = UndoReport::default();
    let mut steps = Vec::new();

    // Kandidaten zuerst sammeln: ein belegter Originalpfad ist unproblematisch,
    // wenn der Beleger selbst zurückbenannt wird (Namenszyklus).
    let candidates = entry
        .renames
        .iter()
        .filter(|rename| {
            if !rename.to.exists() {
                report.skipped.push(UndoSkip {
                    path: rename.to.clone(),
                    reason: "Zieldatei fehlt",
                });
                return false;
            }
            if !rename.target_unchanged() {
                report.skipped.push(UndoSkip {
                    path: rename.to.clone(),
                    reason: "Datei wurde nach dem Umbenennen geändert",
                });
                return false;
            }
            true
        })
        .collect::<Vec<_>>();
    let moving = candidates
        .iter()
        .map(|rename| rename.to.clone())
        .collect::<std::collections::HashSet<_>>();

    for rename in candidates {
        if rename.from.exists() && !moving.contains(&rename.from) {
            report.skipped.push(UndoSkip {
                path: rename.from.clone(),
                reason: "Originalpfad ist belegt",
            });
            continue;
        }
        steps.push(Step {
            current: rename.to.clone(),
            temporary: temporary_path(&rename.to, steps.len()),
            original: rename.from.clone(),
        });
    }

    // Phase 1: aktuelle Namen auf temporäre Namen.
    for (index, step) in steps.iter().enumerate() {
        if let Err(error) = fs::rename(&step.current, &step.temporary) {
            rollback(&steps[..index]);
            return Err(ExecuteError::io(&step.current, error));
        }
    }

    // Phase 2: temporäre Namen auf die Originalnamen.
    for (index, step) in steps.iter().enumerate() {
        if step.original.exists() {
            rollback(&steps[..index]);
            return Err(ExecuteError::TargetExists(step.original.clone()));
        }
        if let Err(error) = fs::rename(&step.temporary, &step.original) {
            rollback(&steps[..index]);
            return Err(ExecuteError::io(&step.original, error));
        }
        report
            .restored
            .push((step.current.clone(), step.original.clone()));
    }

    Ok(report)
}

/// Rollt zurückbenannte Schritte zurück.
fn rollback(steps: &[Step]) {
    for step in steps.iter().rev() {
        if step.temporary.exists() {
            let _ = fs::rename(&step.temporary, &step.current);
        } else if !step.current.exists() && step.original.exists() {
            let _ = fs::rename(&step.original, &step.current);
        }
    }
}

fn metadata_signature(path: &Path) -> Option<(u64, u64, u32)> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    Some((meta.len(), duration.as_secs(), duration.subsec_nanos()))
}

fn temporary_path(source: &Path, index: usize) -> PathBuf {
    let directory = source.parent().unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();
    let mut candidate = directory.join(format!(".fancy-renamer-undo-{pid}-{index}"));
    let mut attempt = 0;
    while candidate.exists() {
        attempt += 1;
        candidate = directory.join(format!(".fancy-renamer-undo-{pid}-{index}-{attempt}"));
    }
    candidate
}
