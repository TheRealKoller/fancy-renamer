//! UI-Tests: benötigen ein Display (lokal via Xvfb, in CI übersprungen).

use std::fs;

use gtk::prelude::*;

use fancy_renamer::engine::{ExifDateRule, ExifMode, MatchMode, ReplaceRule, RuleChain};
use fancy_renamer::ui::file_list;
use fancy_renamer::ui::rule_panel::RulePanel;
use fancy_renamer::ui::state::{AppState, add_paths};
use tempfile::TempDir;

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
    file_list::render(&list, &state.plan);

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
    file_list::render(&list, &state.plan);
    assert_eq!(list.observe_children().n_items(), 2);
}
