//! UI-Tests: benötigen ein Display (lokal via Xvfb, in CI übersprungen).

use std::fs;

use gtk::prelude::*;

use fancy_renamer::engine::{ExifDateRule, ExifMode, MatchMode, ReplaceRule, RuleChain};
use fancy_renamer::ui::file_list;
use fancy_renamer::ui::file_list::RemoveHandler;
use fancy_renamer::ui::rule_panel::RulePanel;
use fancy_renamer::ui::state::{AppState, add_paths, build_plan_async};
use tempfile::TempDir;

fn noop_remove() -> RemoveHandler {
    std::rc::Rc::new(|_| {})
}

fn try_init() -> bool {
    gtk::init().is_ok()
}

#[test]
fn rule_panel_is_empty_by_default() {
    if !try_init() {
        eprintln!("kein Display – Test übersprungen");
        return;
    }
    let panel = RulePanel::new(|| {});
    let chain = panel.build_chain().unwrap();
    assert!(chain.is_empty());
}

#[test]
fn list_renders_group_header_for_pairs() {
    if !try_init() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let paths = ["abc.jpg", "abc.raw", "einzeln.jpg"]
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            fs::write(&path, b"x").unwrap();
            path
        })
        .collect::<Vec<_>>();

    let mut state = AppState {
        items: paths
            .iter()
            .map(|path| fancy_renamer::engine::FileItem::from_path(path).unwrap())
            .collect(),
        chain: Some(RuleChain::new(vec![Box::new(
            ReplaceRule::new(MatchMode::Literal, "abc", "neu", false, false).unwrap(),
        )])),
        ..Default::default()
    };
    state.refresh();

    assert_eq!(state.plan.rename_rows(), 2);
    assert!(state.can_rename());

    let list = gtk::ListBox::new();
    file_list::render(&list, &state.plan, &noop_remove());

    // Gruppenkopf + 2 Paar-Dateien + 1 Einzeldatei = 4 Zeilen
    let mut rows = Vec::new();
    let mut child = list.first_child();
    while let Some(widget) = child {
        rows.push(widget.clone());
        child = widget.next_sibling();
    }
    assert_eq!(rows.len(), 4, "Gruppenkopf und drei Dateien");
}

#[test]
fn add_paths_skips_duplicates() {
    if !try_init() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a.jpg");
    fs::write(&path, b"x").unwrap();
    let state: fancy_renamer::ui::state::SharedState =
        std::rc::Rc::new(std::cell::RefCell::new(AppState::default()));
    add_paths(&state, vec![path.clone(), path.clone()]);
    assert_eq!(state.borrow().items.len(), 1);
}

#[test]
fn rename_is_blocked_by_collision() {
    if !try_init() {
        return;
    }
    let dir = TempDir::new().unwrap();
    for name in ["a.jpg", "b.jpg"] {
        fs::write(dir.path().join(name), b"x").unwrap();
    }
    let mut state = AppState {
        items: ["a.jpg", "b.jpg"]
            .iter()
            .map(|name| fancy_renamer::engine::FileItem::from_path(dir.path().join(name)).unwrap())
            .collect(),
        chain: Some(RuleChain::new(vec![Box::new(
            ReplaceRule::new(MatchMode::Literal, "a", "b", false, false).unwrap(),
        )])),
        ..Default::default()
    };
    state.refresh();
    assert!(!state.can_rename());
    assert_eq!(state.plan.blocking_rows(), 1);
}

#[test]
fn missing_metadata_is_shown_as_skipped() {
    if !try_init() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let photo = dir.path().join("foto.jpg");
    fs::copy("tests/fixtures/exif.jpg", &photo).unwrap();
    let note = dir.path().join("notiz.txt");
    fs::write(&note, b"x").unwrap();

    let mut state = AppState {
        items: [photo, note]
            .iter()
            .map(|path| fancy_renamer::engine::FileItem::from_path(path).unwrap())
            .collect(),
        chain: Some(RuleChain::new(vec![Box::new(ExifDateRule::with_defaults(
            ExifMode::Replace,
        ))])),
        ..Default::default()
    };
    state.refresh();

    assert_eq!(state.plan.rename_rows(), 2);
    let skipped = state
        .plan
        .rows
        .iter()
        .find(|row| row.item.stem == "notiz")
        .unwrap();
    assert_eq!(skipped.status, fancy_renamer::engine::PlanStatus::Skipped);
    assert!(skipped.note.clone().unwrap_or_default().contains("EXIF"));

    let list = gtk::ListBox::new();
    file_list::render(&list, &state.plan, &noop_remove());
    assert_eq!(list.observe_children().n_items(), 2);
}

#[test]
fn remove_button_reports_the_clicked_file() {
    if !try_init() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("weg.jpg");
    fs::write(&path, b"x").unwrap();
    let mut state = AppState {
        items: vec![fancy_renamer::engine::FileItem::from_path(&path).unwrap()],
        ..Default::default()
    };
    state.refresh();

    let removed: std::rc::Rc<std::cell::RefCell<Option<std::path::PathBuf>>> =
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
}

#[test]
fn large_plans_are_computed_in_the_background() {
    if !try_init() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let items = (0..120)
        .map(|index| {
            let path = dir.path().join(format!("f{index:03}.jpg"));
            fs::write(&path, b"x").unwrap();
            fancy_renamer::engine::FileItem::from_path(path).unwrap()
        })
        .collect::<Vec<_>>();
    let groups = fancy_renamer::engine::FileGroup::from_items(&items);
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
}
