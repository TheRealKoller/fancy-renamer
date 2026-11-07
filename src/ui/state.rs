use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::TryRecvError;

use gtk::prelude::*;
use std::time::Duration;

use crate::engine::{
    FileGroup, FileItem, RenamePlan, RuleChain, build_plan, natural_cmp, sort_items,
};

/// Alles, was die Oberfläche anzeigt: Dateiliste, Regelkette und die daraus
/// berechnete Vorschau.
#[derive(Default)]
pub struct AppState {
    pub items: Vec<FileItem>,
    pub chain: Option<RuleChain>,
    pub plan: RenamePlan,
    /// Fehler beim Aufbau der Regelkette (z. B. ungültiges Regex).
    pub chain_error: Option<String>,
    /// Zähler gegen veraltete Ergebnisse aus der Hintergrundberechnung.
    pub plan_generation: u64,
    /// Unterordner beim Hinzufügen von Ordnern einbeziehen.
    pub recursive: bool,
}

pub type SharedState = Rc<RefCell<AppState>>;

impl AppState {
    /// Vorschau aus der aktuellen Regelkette neu berechnen.
    pub fn refresh(&mut self) {
        let groups = FileGroup::from_items(&self.items);
        self.plan = match &self.chain {
            Some(chain) => build_plan(&groups, chain),
            None => build_plan(&groups, &RuleChain::default()),
        };
    }

    pub fn groups(&self) -> Vec<FileGroup> {
        FileGroup::from_items(&self.items)
    }

    pub fn can_rename(&self) -> bool {
        self.chain.is_some() && self.plan.is_executable() && self.plan.rename_rows() > 0
    }
}

/// Dateien übernehmen und natürlich sortieren. Bereits enthaltene Pfade werden
/// still übersprungen; nicht existierende oder nicht lesbare Pfade werden
/// zurückgegeben, damit die Oberfläche sie melden kann.
pub fn add_paths(state: &SharedState, paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut state = state.borrow_mut();
    let mut rejected = Vec::new();
    for path in paths {
        let Ok(canonical) = path.canonicalize() else {
            rejected.push(path);
            continue;
        };
        if state.items.iter().any(|item| item.path == canonical) {
            continue;
        }
        match FileItem::from_path(canonical) {
            Ok(item) => state.items.push(item),
            Err(_) => rejected.push(path),
        }
    }
    sort_items(&mut state.items);
    rejected
}

/// Ergebnis einer Ordner-Suche: gefundene Dateien und nicht lesbare Ordner.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FolderScan {
    pub files: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
}

/// Dateien eines Ordners (nicht rekursiv), ohne versteckte Dateien.
pub fn files_in_folder(folder: &Path) -> Vec<PathBuf> {
    scan_folder(folder, false).files
}

/// Dateien eines Ordners; mit `recursive` auch aus allen Unterordnern.
///
/// Versteckte Dateien und Ordner (führender Punkt) bleiben außen vor.
/// Nicht lesbare Ordner landen in `skipped`, statt den Import abzubrechen.
/// Symlink-Zyklen werden über die Menge der bereits besuchten Verzeichnisse
/// abgefangen, sodass ein Verweis auf einen Elternordner endet.
pub fn scan_folder(folder: &Path, recursive: bool) -> FolderScan {
    let mut scan = FolderScan::default();
    let mut visited = HashSet::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let identity = directory
            .canonicalize()
            .unwrap_or_else(|_| directory.clone());
        if !visited.insert(identity) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&directory) else {
            scan.skipped.push(directory);
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_hidden(&path) {
                continue;
            }
            if path.is_dir() {
                if recursive {
                    pending.push(path);
                }
            } else if path.is_file() {
                scan.files.push(path);
            }
        }
    }
    sort_paths(&mut scan.files);
    scan
}

/// Pfade erweitern: Dateien bleiben, Ordner werden je nach Modus durchsucht.
pub fn expand_paths(paths: impl IntoIterator<Item = PathBuf>, recursive: bool) -> FolderScan {
    let mut scan = FolderScan::default();
    for path in paths {
        if path.is_dir() {
            let nested = scan_folder(&path, recursive);
            scan.files.extend(nested.files);
            scan.skipped.extend(nested.skipped);
        } else {
            scan.files.push(path);
        }
    }
    sort_paths(&mut scan.files);
    scan
}

/// Pfade aus einer Drag-&-Drop-Ablage: Dateien direkt, Ordner über ihren
/// Inhalt (optional samt Unterordnern).
pub fn paths_from_drop(files: &gtk::gdk::FileList, recursive: bool) -> FolderScan {
    let mut scan = FolderScan::default();
    for file in files.files() {
        let Some(path) = file.path() else {
            continue;
        };
        let nested = if path.is_dir() {
            scan_folder(&path, recursive)
        } else {
            FolderScan {
                files: vec![path],
                skipped: Vec::new(),
            }
        };
        scan.files.extend(nested.files);
        scan.skipped.extend(nested.skipped);
    }
    sort_paths(&mut scan.files);
    scan
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

/// Natürliche Reihenfolge über den vollständigen Pfad, damit Läufe
/// reproduzierbar sind.
fn sort_paths(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
}

/// Plan im Hintergrund berechnen und im GTK-Hauptloop abholen. Wird ab etwa
/// 100 Dateien genutzt, damit die Oberfläche bedienbar bleibt.
pub fn build_plan_async(
    groups: Vec<FileGroup>,
    chain: RuleChain,
    on_done: impl Fn(RenamePlan) + 'static,
) {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(build_plan(&groups, &chain));
    });
    gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
        match receiver.try_recv() {
            Ok(plan) => {
                on_done(plan);
                gtk::glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => gtk::glib::ControlFlow::Break,
        }
    });
}
