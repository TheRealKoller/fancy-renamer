use std::cell::RefCell;
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

/// Dateien der obersten Ebene eines Ordners (nicht rekursiv), ohne versteckte
/// Dateien, natürlich sortiert.
pub fn files_in_folder(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut paths = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
        })
        .collect::<Vec<_>>();
    paths.sort_by(|a, b| natural_cmp(&name_of(a), &name_of(b)));
    paths
}

/// Pfade aus einer Drag-&-Drop-Ablage: Dateien direkt, Ordner über ihren
/// Inhalt der obersten Ebene.
pub fn paths_from_drop(files: &gtk::gdk::FileList) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for file in files.files() {
        match file.path() {
            Some(path) if path.is_dir() => paths.extend(files_in_folder(&path)),
            Some(path) => paths.push(path),
            None => {}
        }
    }
    paths
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
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
