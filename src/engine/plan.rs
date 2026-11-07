use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::engine::{FileItem, RuleChain, RuleContext};

/// Bewertung einer Zeile der Vorschau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    /// Datei wird umbenannt.
    Ok,
    /// Umbenennung ändert nur Groß-/Kleinschreibung.
    CaseOnly,
    /// Neuer Name entspricht dem alten; keine Aktion.
    Unchanged,
    /// Ziel existiert bereits und wird nicht selbst umbenannt.
    Collision,
    /// Zwei Zeilen erhalten denselben Zielnamen.
    Duplicate,
    /// Zielname ist unzulässig (leer, Pfadtrenner, NUL, zu lang).
    InvalidName,
    /// Eine Regel konnte nicht angewendet werden (z. B. ungültiges Regex).
    RuleError,
}

impl PlanStatus {
    /// Blockiert die Ausführung des gesamten Plans.
    pub fn is_blocking(self) -> bool {
        matches!(
            self,
            Self::Collision | Self::Duplicate | Self::InvalidName | Self::RuleError
        )
    }

    /// Zeile führt zu einer Umbenennung.
    pub fn is_rename(self) -> bool {
        matches!(self, Self::Ok | Self::CaseOnly)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ok => "wird umbenannt",
            Self::CaseOnly => "nur Groß-/Kleinschreibung",
            Self::Unchanged => "unverändert",
            Self::Collision => "Ziel existiert",
            Self::Duplicate => "doppelter Zielname",
            Self::InvalidName => "ungültiger Name",
            Self::RuleError => "Regelfehler",
        }
    }
}

/// Eine Zeile der Vorschau: eine Datei und ihr geplanter Zielname.
#[derive(Debug, Clone)]
pub struct PlanRow {
    pub item: FileItem,
    pub new_stem: String,
    pub new_path: PathBuf,
    pub status: PlanStatus,
    pub rule_error: Option<String>,
}

/// Ergebnis der Vorschau. Die Ausführung nimmt genau diesen Plan entgegen.
#[derive(Debug, Clone, Default)]
pub struct RenamePlan {
    pub rows: Vec<PlanRow>,
}

impl RenamePlan {
    pub fn blocking_rows(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.status.is_blocking())
            .count()
    }

    pub fn rename_rows(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.status.is_rename())
            .count()
    }

    /// Der Plan darf nur ausgeführt werden, wenn keine Zeile blockiert.
    pub fn is_executable(&self) -> bool {
        self.blocking_rows() == 0
    }
}

/// Berechnet die Vorschau, ohne das Dateisystem zu verändern.
pub fn build_plan(items: &[FileItem], chain: &RuleChain) -> RenamePlan {
    let mut rows: Vec<PlanRow> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let ctx = RuleContext {
                path: &item.path,
                original_stem: &item.stem,
                index,
            };
            match chain.apply(&item.stem, &ctx) {
                Ok(new_stem) => PlanRow {
                    new_path: item.target_path(&new_stem),
                    new_stem,
                    item: item.clone(),
                    status: PlanStatus::Unchanged,
                    rule_error: None,
                },
                Err(error) => PlanRow {
                    new_path: item.path.clone(),
                    new_stem: item.stem.clone(),
                    item: item.clone(),
                    status: PlanStatus::RuleError,
                    rule_error: Some(error.to_string()),
                },
            }
        })
        .collect();

    for row in &mut rows {
        if row.status == PlanStatus::RuleError {
            continue;
        }
        if row.new_path == row.item.path {
            row.status = PlanStatus::Unchanged;
        } else if !is_valid_name(&row.new_stem, &row.item.extension) {
            row.status = PlanStatus::InvalidName;
        } else if is_case_only_change(&row.item.path, &row.new_path) {
            row.status = PlanStatus::CaseOnly;
        } else {
            row.status = PlanStatus::Ok;
        }
    }

    detect_duplicates(&mut rows);
    detect_collisions(&mut rows);

    RenamePlan { rows }
}

/// Gültigkeit von Stamm + Erweiterung: Linux erlaubt in Dateinamen alles außer
/// `/` und NUL, begrenzt die Komponente aber auf 255 Byte.
fn is_valid_name(stem: &str, extension: &str) -> bool {
    if stem.is_empty() || stem == "." || stem == ".." {
        return false;
    }
    if stem.contains('/')
        || stem.contains('\0')
        || extension.contains('/')
        || extension.contains('\0')
    {
        return false;
    }
    let file_name_len = stem.len()
        + if extension.is_empty() {
            0
        } else {
            extension.len() + 1
        };
    file_name_len <= 255
}

fn is_case_only_change(from: &Path, to: &Path) -> bool {
    from.as_os_str() != to.as_os_str()
        && from.as_os_str().to_string_lossy().to_lowercase()
            == to.as_os_str().to_string_lossy().to_lowercase()
}

/// Zwei Zeilen dürfen nicht denselben Zielnamen erhalten.
fn detect_duplicates(rows: &mut [PlanRow]) {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let duplicates: Vec<PathBuf> = rows
        .iter()
        .filter(|row| !row.status.is_blocking())
        .filter(|row| !seen.insert(row.new_path.clone()))
        .map(|row| row.new_path.clone())
        .collect();

    for row in rows {
        if duplicates.contains(&row.new_path) && !row.status.is_blocking() {
            row.status = PlanStatus::Duplicate;
        }
    }
}

/// Ein existierendes Ziel ist nur dann ein Konflikt, wenn es nicht selbst
/// umbenannt wird – sonst wären Namenszyklen (a→b, b→a) unmöglich.
fn detect_collisions(rows: &mut [PlanRow]) {
    let sources_that_move: HashSet<PathBuf> = rows
        .iter()
        .filter(|row| row.status.is_rename())
        .map(|row| row.item.path.clone())
        .collect();

    for row in rows.iter_mut() {
        if !row.status.is_rename() {
            continue;
        }
        let target_exists = row.new_path.exists();
        let target_moves_away = sources_that_move.contains(&row.new_path);
        if target_exists && !target_moves_away {
            row.status = PlanStatus::Collision;
        }
    }
}
