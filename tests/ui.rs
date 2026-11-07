//! UI-Tests. GTK erlaubt Widget-Operationen nur im Thread, der `gtk::init`
//! ausgeführt hat; deshalb laufen alle Testrümpfe über [`ui`] auf einem eigenen
//! GTK-Thread. Ohne Display werden die Tests übersprungen.

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};

use gtk::prelude::*;

use fancy_renamer::engine::{
    ExifDateRule, ExifMode, FileGroup, FileItem, MatchMode, PlanStatus, ReplaceRule, RuleChain,
};
use fancy_renamer::ui::file_list::{self, RemoveHandler};
use fancy_renamer::ui::rule_panel::RulePanel;
use fancy_renamer::ui::state::{AppState, SharedState, add_paths, build_plan_async};
use tempfile::TempDir;

type Job = (Box<dyn FnOnce() + Send>, Sender<std::thread::Result<()>>);

fn ui_thread() -> &'static Sender<Job> {
    static SENDER: OnceLock<Sender<Job>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, receiver) = channel::<Job>();
        std::thread::spawn(move || {
            let has_display = std::env::var_os("DISPLAY").is_some()
                || std::env::var_os("WAYLAND_DISPLAY").is_some();
            let ready = has_display && gtk::init().is_ok();
            if !ready {
                eprintln!("kein Display – UI-Tests übersprungen");
            }
            for (job, done) in receiver {
                if !ready {
                    let _ = done.send(Ok(()));
                    continue;
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                let _ = done.send(result);
            }
        });
        sender
    })
}

/// Testrumpf im GTK-Thread ausführen; ohne Display kehrt der Test sofort zurück.
fn ui(test: impl FnOnce() + Send + 'static) {
    let (done, finished) = channel();
    if ui_thread().send((Box::new(test), done)).is_err() {
        return;
    }
    match finished.recv() {
        Ok(Ok(())) => {}
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(_) => panic!("GTK-Thread hat sich beendet"),
    }
}

fn noop_remove() -> RemoveHandler {
    std::rc::Rc::new(|_| {})
}

fn temp_file(dir: &TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, b"x").unwrap();
    path
}

fn state_with(paths: &[PathBuf], chain: Option<RuleChain>) -> AppState {
    let mut state = AppState {
        items: paths
            .iter()
            .map(|path| FileItem::from_path(path).unwrap())
            .collect(),
        chain,
        ..Default::default()
    };
    state.refresh();
    state
}

#[test]
fn rule_panel_is_empty_by_default() {
    ui(|| {
        let panel = RulePanel::new(|| {});
        assert!(panel.build_chain().unwrap().is_empty());
    });
}

#[test]
fn list_renders_group_header_for_pairs() {
    ui(|| {
        let dir = TempDir::new().unwrap();
        let paths = [
            temp_file(&dir, "abc.jpg"),
            temp_file(&dir, "abc.raw"),
            temp_file(&dir, "einzeln.jpg"),
        ];
        let state = state_with(
            &paths,
            Some(RuleChain::new(vec![Box::new(
                ReplaceRule::new(MatchMode::Literal, "abc", "neu", false, false).unwrap(),
            )])),
        );

        assert_eq!(state.plan.rows.len(), 3);
        assert_eq!(state.plan.rename_rows(), 2, "beide abc-Dateien");
        assert!(state.can_rename());

        let list = gtk::ListBox::new();
        file_list::render(&list, &state.plan, &noop_remove());

        // Gruppenkopf + 2 Paar-Dateien + 1 Einzeldatei = 4 Zeilen
        assert_eq!(list.observe_children().n_items(), 4);
    });
}

#[test]
fn add_paths_skips_duplicates() {
    ui(|| {
        let dir = TempDir::new().unwrap();
        let path = temp_file(&dir, "a.jpg");
        let state: SharedState = std::rc::Rc::new(std::cell::RefCell::new(AppState::default()));
        add_paths(&state, vec![path.clone(), path.clone()]);
        assert_eq!(state.borrow().items.len(), 1);
    });
}

#[test]
fn rename_is_blocked_by_collision() {
    ui(|| {
        let dir = TempDir::new().unwrap();
        let paths = [temp_file(&dir, "a.jpg"), temp_file(&dir, "b.jpg")];
        let state = state_with(
            &paths,
            Some(RuleChain::new(vec![Box::new(
                ReplaceRule::new(MatchMode::Literal, "a", "b", false, false).unwrap(),
            )])),
        );
        assert!(!state.can_rename());
        assert_eq!(state.plan.blocking_rows(), 1);
    });
}

#[test]
fn missing_metadata_is_shown_as_skipped() {
    ui(|| {
        let dir = TempDir::new().unwrap();
        let photo = dir.path().join("foto.jpg");
        fs::copy("tests/fixtures/exif.jpg", &photo).unwrap();
        let note = temp_file(&dir, "notiz.txt");
        let state = state_with(
            &[photo, note],
            Some(RuleChain::new(vec![Box::new(ExifDateRule::with_defaults(
                ExifMode::Replace,
            ))])),
        );

        assert_eq!(state.plan.rows.len(), 2);
        assert_eq!(state.plan.rename_rows(), 1, "nur das Foto hat ein Datum");
        let skipped = state
            .plan
            .rows
            .iter()
            .find(|row| row.item.stem == "notiz")
            .unwrap();
        assert_eq!(skipped.status, PlanStatus::Skipped);
        assert!(skipped.note.clone().unwrap_or_default().contains("EXIF"));

        let list = gtk::ListBox::new();
        file_list::render(&list, &state.plan, &noop_remove());
        assert_eq!(list.observe_children().n_items(), 2);
    });
}

#[test]
fn remove_button_reports_the_clicked_file() {
    ui(|| {
        let dir = TempDir::new().unwrap();
        let path = temp_file(&dir, "weg.jpg");
        let state = state_with(std::slice::from_ref(&path), None);

        let removed: std::rc::Rc<std::cell::RefCell<Option<PathBuf>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        let recorder = removed.clone();
        let handler: RemoveHandler = std::rc::Rc::new(move |clicked| {
            *recorder.borrow_mut() = Some(clicked.to_path_buf());
        });

        let list = gtk::ListBox::new();
        file_list::render(&list, &state.plan, &handler);

        let row = list
            .first_child()
            .unwrap()
            .downcast::<gtk::ListBoxRow>()
            .unwrap();
        let line = row.child().unwrap();
        let button = line
            .last_child()
            .unwrap()
            .downcast::<gtk::Button>()
            .unwrap();
        button.emit_clicked();

        assert_eq!(removed.borrow().as_deref(), Some(path.as_path()));
    });
}

#[test]
fn large_plans_are_computed_in_the_background() {
    ui(|| {
        let dir = TempDir::new().unwrap();
        let paths = (0..120)
            .map(|index| temp_file(&dir, &format!("f{index:03}.jpg")))
            .collect::<Vec<_>>();
        let items = paths
            .iter()
            .map(|path| FileItem::from_path(path).unwrap())
            .collect::<Vec<_>>();
        let groups = FileGroup::from_items(&items);
        let chain = RuleChain::new(vec![Box::new(
            ReplaceRule::new(MatchMode::Literal, "f", "bild", false, false).unwrap(),
        )]);

        let done = std::rc::Rc::new(std::cell::Cell::new(false));
        let flag = done.clone();
        build_plan_async(groups, chain, move |plan| {
            assert_eq!(plan.rename_rows(), 120);
            flag.set(true);
        });

        let context = gtk::glib::MainContext::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !done.get() && std::time::Instant::now() < deadline {
            context.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(done.get(), "Hintergrundberechnung lieferte kein Ergebnis");
    });
}
