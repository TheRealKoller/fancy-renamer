//! UI-Tests. GTK erlaubt Widget-Operationen nur im Thread, der `gtk::init`
//! ausgeführt hat; deshalb laufen alle Testrümpfe über [`ui`] auf einem eigenen
//! GTK-Thread. Ohne Display werden die Tests übersprungen.

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::mpsc::{Sender, channel};

use gtk::prelude::*;
use libadwaita::prelude::*;

use fancy_renamer::engine::{
    ExifDateRule, ExifMode, FileGroup, FileItem, MatchMode, PlanStatus, ReplaceRule, RuleChain,
};
use fancy_renamer::ui::file_list::{self, RemoveHandler};
use fancy_renamer::ui::rule_panel::RulePanel;
use fancy_renamer::ui::state::{
    AppState, SharedState, add_paths, build_plan_async, files_in_folder, paths_from_drop,
};
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

/// Zeilentitel sind Pango-Markup; für Vergleiche den reinen Text.
fn plain_title(markup: &str) -> String {
    match gtk::pango::parse_markup(markup, '\0') {
        Ok((_, text, _)) => text.to_string(),
        Err(_) => markup.to_string(),
    }
}

fn rule_titles(panel: &RulePanel) -> Vec<String> {
    let mut titles = Vec::new();
    let mut child = panel.widget().first_child();
    while let Some(widget) = child {
        let widget: &gtk::Widget = widget.upcast_ref();
        if let Some(row) = widget.downcast_ref::<libadwaita::ExpanderRow>() {
            titles.push(plain_title(&row.title()));
        }
        child = widget.next_sibling();
    }
    titles
}

fn find_expander(widget: &impl IsA<gtk::Widget>, title: &str) -> Option<libadwaita::ExpanderRow> {
    let widget: &gtk::Widget = widget.upcast_ref();
    if let Some(row) = widget.downcast_ref::<libadwaita::ExpanderRow>() {
        if plain_title(&row.title()) == title {
            return Some(row.clone());
        }
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        if let Some(found) = find_expander(&node, title) {
            return Some(found);
        }
        child = node.next_sibling();
    }
    None
}

fn find_button(widget: &impl IsA<gtk::Widget>, tooltip: &str) -> Option<gtk::Button> {
    let widget: &gtk::Widget = widget.upcast_ref();
    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
        if button.tooltip_text().as_deref() == Some(tooltip) {
            return Some(button.clone());
        }
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        if let Some(found) = find_button(&node, tooltip) {
            return Some(found);
        }
        child = node.next_sibling();
    }
    None
}

/// Enthält der Teilbaum ein sichtbares Label mit diesem Text?
fn label_visible(widget: &impl IsA<gtk::Widget>, needle: &str) -> bool {
    let widget: &gtk::Widget = widget.upcast_ref();
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        if label.text().contains(needle) && label.is_visible() {
            return true;
        }
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        if label_visible(&node, needle) {
            return true;
        }
        child = node.next_sibling();
    }
    false
}

fn find_dropdown(widget: &impl IsA<gtk::Widget>) -> Option<gtk::DropDown> {
    let widget: &gtk::Widget = widget.upcast_ref();
    if let Some(found) = widget.downcast_ref::<gtk::DropDown>() {
        return Some(found.clone());
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        if let Some(found) = find_dropdown(&node) {
            return Some(found);
        }
        child = node.next_sibling();
    }
    None
}

fn find_switch(widget: &impl IsA<gtk::Widget>) -> Option<gtk::Switch> {
    let widget: &gtk::Widget = widget.upcast_ref();
    if let Some(found) = widget.downcast_ref::<gtk::Switch>() {
        return Some(found.clone());
    }
    let mut child = widget.first_child();
    while let Some(node) = child {
        if let Some(found) = find_switch(&node) {
            return Some(found);
        }
        child = node.next_sibling();
    }
    None
}

/// Schrittnummer, die eine Karte anzeigt.
fn badge_text(row: &libadwaita::ExpanderRow) -> String {
    fn walk(widget: &gtk::Widget) -> Option<String> {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if label.has_css_class("numeric") {
                return Some(label.text().to_string());
            }
        }
        let mut child = widget.first_child();
        while let Some(node) = child {
            if let Some(found) = walk(&node) {
                return Some(found);
            }
            child = node.next_sibling();
        }
        None
    }
    walk(row.upcast_ref()).unwrap_or_default()
}

#[test]
fn numbering_appends_by_default_so_earlier_rules_survive() {
    ui(|| {
        let panel = RulePanel::new(|| {});
        let numbering = find_expander(panel.widget(), "Nummerierung").unwrap();
        let mode = find_dropdown(&numbering).unwrap();
        assert_eq!(mode.selected(), 1);
        assert!(!label_visible(&numbering, "verwirft"));

        // „Stamm ersetzen“ blendet den Hinweis ein, dass vorherige Regeln
        // verworfen werden.
        mode.set_selected(0);
        assert!(label_visible(&numbering, "verwirft"));
        mode.set_selected(1);
        assert!(!label_visible(&numbering, "verwirft"));
    });
}

#[test]
fn active_rules_show_their_step_number_in_chain_order() {
    ui(|| {
        let panel = RulePanel::new(|| {});
        let replace = find_expander(panel.widget(), "Suchen & Ersetzen").unwrap();
        let numbering = find_expander(panel.widget(), "Nummerierung").unwrap();
        assert_eq!(badge_text(&replace), "–");

        find_switch(&numbering).unwrap().set_active(true);
        assert_eq!(badge_text(&numbering), "1");
        assert_eq!(badge_text(&replace), "–");

        // Suchen & Ersetzen steht über Nummerierung: nach dem Aktivieren
        // bekommt es die 1, die Nummerierung rückt auf 2.
        find_switch(&replace).unwrap().set_active(true);
        assert_eq!(badge_text(&replace), "1");
        assert_eq!(badge_text(&numbering), "2");

        // Verschieben ändert die Anwendungsreihenfolge und damit die Nummern.
        find_button(&numbering, "Regel nach oben")
            .unwrap()
            .emit_clicked();
        assert_eq!(rule_titles(&panel)[0], "Nummerierung");
        assert_eq!(badge_text(&numbering), "1");
        assert_eq!(badge_text(&replace), "2");
    });
}

#[test]
fn rules_move_step_by_step_in_both_directions() {
    ui(|| {
        let panel = RulePanel::new(|| {});
        assert_eq!(
            rule_titles(&panel),
            [
                "Suchen & Ersetzen",
                "Nummerierung",
                "Namen normalisieren",
                "Aufnahmedatum (EXIF)"
            ]
        );

        let exif = find_expander(panel.widget(), "Aufnahmedatum (EXIF)").unwrap();
        for _ in 0..3 {
            find_button(&exif, "Regel nach oben")
                .unwrap()
                .emit_clicked();
        }
        assert_eq!(rule_titles(&panel)[0], "Aufnahmedatum (EXIF)");

        // Nach dem Verschieben bleibt die Karte bedienbar, obwohl sich ihre
        // Position geändert hat.
        find_button(&exif, "Regel nach unten")
            .unwrap()
            .emit_clicked();
        assert_eq!(rule_titles(&panel)[1], "Aufnahmedatum (EXIF)");

        // An Position 1 sind beide Richtungen möglich.
        let exif = find_expander(panel.widget(), "Aufnahmedatum (EXIF)").unwrap();
        assert!(
            find_button(&exif, "Regel nach oben")
                .unwrap()
                .is_sensitive()
        );
        assert!(
            find_button(&exif, "Regel nach unten")
                .unwrap()
                .is_sensitive()
        );

        // Ränder sind No-Ops, die Buttons dort sind deaktiviert.
        let top_title = rule_titles(&panel)[0].clone();
        let top = find_expander(panel.widget(), &top_title).unwrap();
        let up = find_button(&top, "Regel nach oben").unwrap();
        assert!(!up.is_sensitive());
        up.emit_clicked();
        assert_eq!(rule_titles(&panel)[0], top_title);

        let bottom_title = rule_titles(&panel)[3].clone();
        let bottom = find_expander(panel.widget(), &bottom_title).unwrap();
        let down = find_button(&bottom, "Regel nach unten").unwrap();
        assert!(!down.is_sensitive());
        down.emit_clicked();
        assert_eq!(rule_titles(&panel)[3], bottom_title);
    });
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
fn added_files_are_sorted_naturally() {
    ui(|| {
        let dir = TempDir::new().unwrap();
        let paths = vec![
            temp_file(&dir, "foto10.jpg"),
            temp_file(&dir, "foto2.jpg"),
            temp_file(&dir, "Foto1.jpg"),
        ];
        let state: SharedState = std::rc::Rc::new(std::cell::RefCell::new(AppState::default()));
        assert!(add_paths(&state, paths).is_empty());
        let names = state
            .borrow()
            .items
            .iter()
            .map(|item| item.file_name_for(&item.stem))
            .collect::<Vec<_>>();
        assert_eq!(names, ["Foto1.jpg", "foto2.jpg", "foto10.jpg"]);
    });
}

#[test]
fn folder_import_takes_top_level_files_only() {
    ui(|| {
        let dir = TempDir::new().unwrap();
        temp_file(&dir, "b.jpg");
        temp_file(&dir, "a.jpg");
        temp_file(&dir, ".versteckt.jpg");
        fs::create_dir(dir.path().join("unterordner")).unwrap();

        let paths = files_in_folder(dir.path());
        let names = paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, ["a.jpg", "b.jpg"]);

        let state: SharedState = std::rc::Rc::new(std::cell::RefCell::new(AppState::default()));
        assert!(add_paths(&state, paths).is_empty());
        assert_eq!(state.borrow().items.len(), 2);
    });
}

#[test]
fn dropped_folders_contribute_their_files() {
    ui(|| {
        let dir = TempDir::new().unwrap();
        let folder = dir.path().join("ordner");
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("b.jpg"), b"x").unwrap();
        fs::write(folder.join("a.jpg"), b"x").unwrap();
        let single = temp_file(&dir, "einzeln.jpg");

        let files = gtk::gdk::FileList::from_array(&[
            gtk::gio::File::for_path(&folder),
            gtk::gio::File::for_path(&single),
        ]);
        let paths = paths_from_drop(&files);
        let names = paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, ["a.jpg", "b.jpg", "einzeln.jpg"]);
    });
}

#[test]
fn missing_paths_are_reported_and_not_added() {
    ui(|| {
        let dir = TempDir::new().unwrap();
        let existing = temp_file(&dir, "da.jpg");
        let missing = dir.path().join("weg.jpg");
        let state: SharedState = std::rc::Rc::new(std::cell::RefCell::new(AppState::default()));
        let rejected = add_paths(&state, vec![existing, missing.clone()]);
        assert_eq!(rejected, vec![missing]);
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

#[test]
fn loading_a_preset_replaces_chain_order_and_parameters() {
    ui(|| {
        use fancy_renamer::engine::{CaseMode, NumberingMode, Preset, RuleOutcome, RulePreset};

        let panel = RulePanel::new(|| {});
        let photos = Preset {
            name: "Fotos".into(),
            rules: vec![
                RulePreset::Normalize {
                    case: CaseMode::Upper,
                    spaces_to_underscore: false,
                    trim_separators: false,
                    transliterate: false,
                    strip_special: false,
                },
                RulePreset::Numbering {
                    mode: NumberingMode::Append,
                    start: 5,
                    step: 2,
                    digits: 4,
                    prefix: "-".into(),
                    suffix: String::new(),
                },
            ],
        };
        panel.apply_preset(&photos);
        assert_eq!(panel.snapshot(), photos.rules);
        let chain = panel.build_chain().unwrap();
        assert_eq!(chain.ids(), vec!["normalize", "numbering"]);
        let context = fancy_renamer::engine::RuleContext {
            path: std::path::Path::new("foto.jpg"),
            original_stem: "foto",
            index: 0,
            group_index: 0,
            group_size: 1,
            member_index: 0,
        };
        assert_eq!(
            chain.apply("foto", &context).unwrap(),
            RuleOutcome::Stem("FOTO-0005".into())
        );

        // Ein Preset mit nur einer Regel ersetzt die Kette vollständig.
        let single = Preset {
            name: "Nur ersetzen".into(),
            rules: vec![RulePreset::Replace {
                mode: MatchMode::Literal,
                pattern: "IMG".into(),
                replacement: "Foto".into(),
                case_insensitive: false,
                first_only: false,
            }],
        };
        panel.apply_preset(&single);
        assert_eq!(panel.snapshot(), single.rules);
        assert_eq!(panel.build_chain().unwrap().ids(), vec!["replace"]);
    });
}

#[test]
fn preset_menu_lists_saved_presets() {
    ui(|| {
        use fancy_renamer::engine::{Preset, Presets};

        let mut presets = Presets::default();
        presets
            .upsert(Preset {
                name: "Fotos".into(),
                rules: vec![],
            })
            .unwrap();
        presets
            .upsert(Preset {
                name: "Musik".into(),
                rules: vec![],
            })
            .unwrap();
        assert_eq!(
            fancy_renamer::ui::presets::entry_labels(&presets),
            vec!["Fotos".to_string(), "Musik".to_string()]
        );
        assert_eq!(
            format!("win.{}", fancy_renamer::ui::presets::LOAD_ACTION_NAME),
            fancy_renamer::ui::presets::LOAD_ACTION
        );
        let menu = fancy_renamer::ui::presets::menu(&presets);
        // Abschnitt mit Einträgen plus Verwaltungsabschnitt.
        assert_eq!(menu.n_items(), 2);

        let empty = fancy_renamer::ui::presets::menu(&Presets::default());
        assert_eq!(empty.n_items(), 2);

        // Der Menüknopf akzeptiert das Modell.
        let button = gtk::MenuButton::new();
        button.set_menu_model(Some(&menu));
        assert!(button.menu_model().is_some());
    });
}
