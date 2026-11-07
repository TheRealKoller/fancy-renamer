use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::engine::{FileGroup, FileItem, RenamePlan, RuleChain, build_plan};

/// Alles, was die Oberfläche anzeigt: Dateiliste, Regelkette und die daraus
/// berechnete Vorschau.
#[derive(Default)]
pub struct AppState {
    pub items: Vec<FileItem>,
    pub chain: Option<RuleChain>,
    pub plan: RenamePlan,
    /// Fehler beim Aufbau der Regelkette (z. B. ungültiges Regex).
    pub chain_error: Option<String>,
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

/// Dateien übernehmen; bereits enthaltene Pfade werden übersprungen.
pub fn add_paths(state: &SharedState, paths: impl IntoIterator<Item = PathBuf>) {
    let mut state = state.borrow_mut();
    for path in paths {
        let path = path.canonicalize().unwrap_or(path);
        if state.items.iter().any(|item| item.path == path) {
            continue;
        }
        if let Ok(item) = FileItem::from_path(path) {
            state.items.push(item);
        }
    }
}

/// Dateien der obersten Ebene eines Ordners, ohne versteckte Dateien.
pub fn files_in_folder(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
        })
        .collect()
}
