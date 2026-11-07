use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use gtk::gio;
use gtk::prelude::*;
use libadwaita::prelude::*;

use crate::engine::{
    ExecutionReport, FileItem, Journal, JournalEntry, PRESET_FILE, Preset, Presets, RenamePlan,
    build_plan, execute, undo, validate_name,
};

use super::presets as presets_menu;
use super::rule_panel::RulePanel;
use super::state::{AppState, SharedState};
use super::{file_list, state};

const APP_ID: &str = "io.github.therealKoller.FancyRenamer";

/// Callback-Slot, über den der Regelbereich die Vorschau auslöst.
type ChangeSlot = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// Übernimmt Pfade, die von außen hereinkommen (Kommandozeile, Dateimanager).
type AddPaths = Rc<dyn Fn(Vec<PathBuf>)>;

thread_local! {
    static ADD_CALLBACK: RefCell<Option<AddPaths>> = const { RefCell::new(None) };
}

fn add_callback() -> Option<AddPaths> {
    ADD_CALLBACK.with(|slot| slot.borrow().clone())
}

/// Pfade, die beim Start bereits übergeben wurden, und ob Unterordner
/// einbezogen werden (`--recursive`/`-r`).
pub fn parse_args(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> (Vec<PathBuf>, bool) {
    let mut paths = Vec::new();
    let mut recursive = false;
    for argument in arguments {
        let text = argument.to_string_lossy();
        if text == "--recursive" || text == "-r" {
            recursive = true;
        } else if !text.starts_with('-') {
            paths.push(PathBuf::from(argument));
        }
    }
    (paths, recursive)
}

fn pending_paths() -> (Vec<PathBuf>, bool) {
    parse_args(std::env::args_os().skip(1))
}

/// Callback mit Ergebnis der Ausführung.
type FinishCallback = Rc<dyn Fn(Result<ExecutionReport, crate::engine::ExecuteError>)>;

pub fn run() -> Result<()> {
    let app = libadwaita::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_startup(|app| {
        // Rückmeldung für Drop-Ziele.
        let css = gtk::CssProvider::new();
        css.load_from_string(
            ".drop-highlight { background-color: alpha(@accent_bg_color, 0.25); }",
        );
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        for (action, accels) in [
            ("win.open", vec!["<Control>o"]),
            ("win.rename", vec!["<Control>Return"]),
            ("win.clear", vec!["<Control>Delete"]),
            ("win.undo", vec!["<Control>z"]),
        ] {
            app.set_accels_for_action(action, &accels);
        }
    });
    app.connect_activate(|app| {
        let (pending, recursive) = pending_paths();
        build_window(app, pending, recursive);
    });
    app.connect_open(|app, files, _hint| {
        if app.active_window().is_none() {
            build_window(app, Vec::new(), false);
        }
        let paths = files
            .iter()
            .filter_map(|file| file.path())
            .collect::<Vec<_>>();
        if let Some(add) = add_callback() {
            add(paths);
        }
    });

    // Eigene Argumente (Pfade, `--recursive`) selbst auswerten; GApplication
    // würde unbekannte Optionen ablehnen.
    let program = std::env::args()
        .next()
        .unwrap_or_else(|| "fancy-renamer".to_string());
    app.run_with_args(&[program.as_str()]);
    Ok(())
}

fn build_window(app: &libadwaita::Application, initial: Vec<PathBuf>, initial_recursive: bool) {
    let state: SharedState = Rc::new(RefCell::new(AppState::default()));

    let header = libadwaita::HeaderBar::new();
    let add_button = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Dateien hinzufügen (Ctrl+O)")
        .build();
    let folder_button = gtk::Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text("Ordner hinzufügen")
        .build();
    let recursive_toggle = gtk::ToggleButton::builder()
        .label("Unterordner")
        .active(initial_recursive)
        .tooltip_text(
            "Ordner samt Unterordnern einbeziehen – gilt für Ordnerauswahl, \
             Drag & Drop und Kommandozeile",
        )
        .build();
    state.borrow_mut().recursive = initial_recursive;
    let clear_button = gtk::Button::builder()
        .icon_name("edit-clear-all-symbolic")
        .tooltip_text("Alle entfernen (Ctrl+Entf)")
        .build();
    header.pack_start(&add_button);
    header.pack_start(&folder_button);
    header.pack_start(&recursive_toggle);
    header.pack_end(&clear_button);

    // Regelbereich
    let fire: ChangeSlot = Rc::new(RefCell::new(None));
    let fire_for_panel = fire.clone();
    let panel = Rc::new(RulePanel::new(move || {
        if let Some(callback) = fire_for_panel.borrow().as_ref() {
            callback();
        }
    }));

    {
        let state = state.clone();
        recursive_toggle.connect_toggled(move |button| {
            state.borrow_mut().recursive = button.is_active();
        });
    }

    // Dateiliste
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .valign(gtk::Align::Start)
        .build();
    list.add_css_class("boxed-list");
    let empty_state = libadwaita::StatusPage::builder()
        .title("Keine Dateien")
        .description("Dateien oder Ordner hinzufügen, um sie umzubenennen.")
        .icon_name("folder-open-symbolic")
        .vexpand(true)
        .build();
    let list_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    let stack = gtk::Stack::new();
    stack.add_named(&empty_state, Some("empty"));
    stack.add_named(&list_scroll, Some("list"));
    stack.set_visible_child_name("empty");
    stack.set_vexpand(true);

    // Aktionsleiste
    let summary = gtk::Label::new(Some("Keine Dateien"));
    summary.set_xalign(0.0);
    summary.add_css_class("dim-label");
    summary.set_hexpand(true);
    summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let undo_button = gtk::Button::builder()
        .icon_name("edit-undo-symbolic")
        .tooltip_text("Letzte Umbenennung rückgängig machen (Ctrl+Z)")
        .sensitive(false)
        .build();
    let rename_button = gtk::Button::with_label("Umbenennen");
    rename_button.add_css_class("suggested-action");
    rename_button.set_sensitive(false);
    rename_button.set_tooltip_text(Some("Vorschau ist noch leer"));
    let action_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    action_bar.add_css_class("toolbar");
    action_bar.set_margin_top(8);
    action_bar.set_margin_bottom(8);
    action_bar.set_margin_start(12);
    action_bar.set_margin_end(12);
    action_bar.append(&summary);
    action_bar.append(&undo_button);
    action_bar.append(&rename_button);

    // Regeln und Vorschau teilen sich den Platz; die Trennlinie ist ziehbar,
    // damit der Einstellungsbereich bei Bedarf wachsen kann.
    let rules_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(panel.widget())
        .build();
    let paned = gtk::Paned::new(gtk::Orientation::Vertical);
    paned.set_start_child(Some(&rules_scroll));
    paned.set_end_child(Some(&stack));
    paned.set_resize_start_child(true);
    paned.set_resize_end_child(true);
    paned.set_wide_handle(true);
    paned.set_position(460);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&paned);

    let toolbar_view = libadwaita::ToolbarView::builder().content(&content).build();
    toolbar_view.add_top_bar(&header);
    toolbar_view.add_bottom_bar(&action_bar);

    let toast_overlay = libadwaita::ToastOverlay::new();
    toast_overlay.set_child(Some(&toolbar_view));

    // Übersprungene Pfade melden (nicht vorhanden, nicht lesbar).
    let report_skipped: Rc<dyn Fn(Vec<PathBuf>)> = {
        let toast_overlay = toast_overlay.clone();
        Rc::new(move |paths: Vec<PathBuf>| {
            if paths.is_empty() {
                return;
            }
            let text = match paths.len() {
                1 => format!("Nicht aufgenommen: {}", paths[0].display()),
                count => format!("{count} Pfad(e) nicht aufgenommen (nicht lesbar)"),
            };
            toast_overlay.add_toast(libadwaita::Toast::new(&text));
        })
    };

    let window = libadwaita::ApplicationWindow::builder()
        .application(app)
        .title("fancy-renamer")
        .default_width(1100)
        .default_height(800)
        .content(&toast_overlay)
        .build();

    // Inhalt der Vorschau aus dem Zustand zeichnen.
    let refresh_slot: ChangeSlot = Rc::new(RefCell::new(None));
    let remove: file_list::RemoveHandler = {
        let state = state.clone();
        let refresh_slot = refresh_slot.clone();
        Rc::new(move |path| {
            state.borrow_mut().items.retain(|item| item.path != path);
            if let Some(refresh) = refresh_slot.borrow().as_ref() {
                refresh();
            }
        })
    };
    let render: Rc<dyn Fn()> = {
        let state = state.clone();
        let list = list.clone();
        let stack = stack.clone();
        let summary = summary.clone();
        let rename_button = rename_button.clone();
        let remove = remove.clone();
        Rc::new(move || {
            let state = state.borrow();
            file_list::render(&list, &state.plan, &remove);
            summary.set_text(&summary_text(&state));
            rename_button.set_sensitive(state.can_rename());
            rename_button.set_tooltip_text(Some(&tooltip(&state)));
            stack.set_visible_child_name(if state.items.is_empty() {
                "empty"
            } else {
                "list"
            });
        })
    };

    // Vorschau neu berechnen: Regeln einlesen, Plan bauen (bei vielen Dateien
    // im Hintergrund), dann zeichnen.
    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let panel = panel.clone();
        let render = render.clone();
        Rc::new(move || {
            let (groups, chain, generation) = {
                let mut state = state.borrow_mut();
                match panel.build_chain() {
                    Ok(chain) => {
                        state.chain = Some(chain);
                        state.chain_error = None;
                    }
                    Err(error) => {
                        state.chain = None;
                        state.chain_error = Some(error);
                    }
                }
                state.plan_generation += 1;
                (state.groups(), state.chain.clone(), state.plan_generation)
            };
            panel.show_error(state.borrow().chain_error.as_deref());

            let Some(chain) = chain else {
                state.borrow_mut().plan = RenamePlan::default();
                render();
                return;
            };

            if groups.len() > 100 {
                let state = state.clone();
                let render = render.clone();
                state::build_plan_async(groups, chain, move |plan| {
                    let mut state = state.borrow_mut();
                    if state.plan_generation != generation {
                        return;
                    }
                    state.plan = plan;
                    drop(state);
                    render();
                });
            } else {
                let plan = build_plan(&groups, &chain);
                state.borrow_mut().plan = plan;
                render();
            }
        })
    };
    *refresh_slot.borrow_mut() = Some(refresh.clone());

    // Entprellung: höchstens alle 150 ms neu rechnen
    let pending = Rc::new(Cell::new(false));
    let refresh_for_debounce = refresh.clone();
    let debounce: Rc<dyn Fn()> = {
        let pending = pending.clone();
        Rc::new(move || {
            if pending.get() {
                return;
            }
            pending.set(true);
            let refresh = refresh_for_debounce.clone();
            let pending = pending.clone();
            gtk::glib::timeout_add_local_once(Duration::from_millis(150), move || {
                pending.set(false);
                refresh();
            });
        })
    };
    *fire.borrow_mut() = Some(debounce.clone());

    // Dateien hinzufügen
    let add_files: Rc<dyn Fn()> =
        {
            let window = window.clone();
            let state = state.clone();
            let refresh = refresh.clone();
            let report_skipped = report_skipped.clone();
            Rc::new(move || {
                let dialog = gtk::FileDialog::builder()
                    .title("Dateien auswählen")
                    .modal(true)
                    .build();
                let state = state.clone();
                let refresh = refresh.clone();
                let report_skipped = report_skipped.clone();
                dialog.open_multiple(Some(&window), gio::Cancellable::NONE, move |result| {
                    match result {
                        Ok(files) => {
                            let paths = (0..files.n_items()).filter_map(|index| {
                                files
                                    .item(index)
                                    .and_then(|object| object.downcast::<gio::File>().ok())
                                    .and_then(|file| file.path())
                            });
                            let skipped = state::add_paths(&state, paths);
                            report_skipped.as_ref()(skipped);
                            refresh();
                        }
                        Err(error) => {
                            eprintln!("Dateiauswahl abgebrochen: {error}");
                        }
                    }
                });
            })
        };

    let add_folder: Rc<dyn Fn()> =
        {
            let window = window.clone();
            let state = state.clone();
            let refresh = refresh.clone();
            let report_skipped = report_skipped.clone();
            Rc::new(move || {
                let dialog = gtk::FileDialog::builder()
                    .title("Ordner auswählen")
                    .modal(true)
                    .build();
                let state = state.clone();
                let refresh = refresh.clone();
                let report_skipped = report_skipped.clone();
                dialog.select_folder(Some(&window), gio::Cancellable::NONE, move |result| {
                    match result {
                        Ok(folder) => {
                            if let Some(path) = folder.path() {
                                let recursive = state.borrow().recursive;
                                let scan = state::scan_folder(&path, recursive);
                                let mut skipped = scan.skipped;
                                skipped.extend(state::add_paths(&state, scan.files));
                                report_skipped.as_ref()(skipped);
                                refresh();
                            }
                        }
                        Err(error) => {
                            eprintln!("Ordnerauswahl abgebrochen: {error}");
                        }
                    }
                });
            })
        };

    // Journal der letzten Läufe und Rückgängig-Funktion.
    let journal_path = gtk::glib::user_state_dir()
        .join("fancy-renamer")
        .join("journal.json");
    let journal: Rc<RefCell<Journal>> = Rc::new(RefCell::new(Journal::load(&journal_path)));
    let sync_undo = {
        let journal = journal.clone();
        let undo_button = undo_button.clone();
        Rc::new(move || undo_button.set_sensitive(!journal.borrow().is_empty()))
    };
    let undo_last: Rc<dyn Fn()> = {
        let state = state.clone();
        let refresh = refresh.clone();
        let journal = journal.clone();
        let journal_path = journal_path.clone();
        let toast_overlay = toast_overlay.clone();
        let sync_undo = sync_undo.clone();
        Rc::new(move || {
            let Some(entry) = journal.borrow().last().cloned() else {
                toast_overlay.add_toast(libadwaita::Toast::new(
                    "Es gibt keine Umbenennung zum Rückgängigmachen",
                ));
                sync_undo();
                return;
            };
            let result = undo(&entry);
            match result {
                Ok(report) => {
                    {
                        let mut journal = journal.borrow_mut();
                        journal.pop();
                        if let Err(error) = journal.save(&journal_path) {
                            toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                                "Journal nicht schreibbar: {error}"
                            )));
                        }
                    }
                    {
                        let mut state = state.borrow_mut();
                        for (current, original) in &report.restored {
                            if let Some(index) =
                                state.items.iter().position(|item| &item.path == current)
                            {
                                if let Ok(item) = FileItem::from_path(original.clone()) {
                                    state.items[index] = item;
                                }
                            }
                        }
                    }
                    refresh();
                    let mut text = format!("{} Datei(en) zurückbenannt", report.restored_count());
                    if let Some(first) = report.skipped.first() {
                        text.push_str(&format!(
                            ", {} übersprungen ({})",
                            report.skipped.len(),
                            first.reason
                        ));
                    }
                    toast_overlay.add_toast(libadwaita::Toast::new(&text));
                    sync_undo();
                }
                Err(error) => {
                    toast_overlay.add_toast(libadwaita::Toast::new(&format!("Fehler: {error}")));
                }
            }
        })
    };
    sync_undo();
    let undo_for_button = undo_last.clone();
    undo_button.connect_clicked(move |_| undo_for_button());

    let clear_all: Rc<dyn Fn()> = {
        let state = state.clone();
        let refresh = refresh.clone();
        Rc::new(move || {
            state.borrow_mut().items.clear();
            refresh();
        })
    };

    // Presets: benannte Regelketten in XDG_CONFIG_HOME/fancy-renamer/presets.json
    let presets_path = gtk::glib::user_config_dir()
        .join("fancy-renamer")
        .join(PRESET_FILE);
    let presets: Rc<RefCell<Presets>> = Rc::new(RefCell::new(Presets::load(&presets_path)));
    let persist_presets: Rc<dyn Fn()> = {
        let presets = presets.clone();
        let presets_path = presets_path.clone();
        let toast_overlay = toast_overlay.clone();
        Rc::new(move || {
            if let Err(error) = presets.borrow().save(&presets_path) {
                toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                    "Presets nicht schreibbar: {error}"
                )));
            }
        })
    };
    let rebuild_presets_menu: Rc<dyn Fn()> = {
        let presets = presets.clone();
        let button = panel.presets_button().clone();
        Rc::new(move || {
            let menu = presets_menu::menu(&presets.borrow());
            button.set_menu_model(Some(&menu));
        })
    };
    rebuild_presets_menu();

    // Aktuelle Kette unter einem Namen ablegen; vorhandene Namen nach Rückfrage ersetzen.
    let save_current_preset: Rc<dyn Fn()> = {
        let window = window.clone();
        let panel = panel.clone();
        let presets = presets.clone();
        let persist_presets = persist_presets.clone();
        let rebuild_presets_menu = rebuild_presets_menu.clone();
        let toast_overlay = toast_overlay.clone();
        Rc::new(move || {
            let rules = panel.snapshot();
            if rules.is_empty() {
                toast_overlay.add_toast(libadwaita::Toast::new("Keine aktive Regel zum Speichern"));
                return;
            }
            let entry = gtk::Entry::builder()
                .placeholder_text("Name")
                .activates_default(true)
                .build();
            let dialog = libadwaita::AlertDialog::builder()
                .heading("Preset speichern")
                .body("Name für die aktuelle Regelkette:")
                .extra_child(&entry)
                .build();
            dialog.add_response("cancel", "Abbrechen");
            dialog.add_response("save", "Speichern");
            dialog.set_response_appearance("save", libadwaita::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("save"));
            dialog.set_close_response("cancel");

            let write: Rc<dyn Fn(String)> = {
                let presets = presets.clone();
                let persist_presets = persist_presets.clone();
                let rebuild_presets_menu = rebuild_presets_menu.clone();
                let toast_overlay = toast_overlay.clone();
                let rules = rules.clone();
                Rc::new(move |name: String| {
                    let result = presets.borrow_mut().upsert(Preset {
                        name: name.clone(),
                        rules: rules.clone(),
                    });
                    match result {
                        Ok(replaced) => {
                            persist_presets();
                            rebuild_presets_menu();
                            let text = if replaced {
                                format!("Preset „{name}“ ersetzt")
                            } else {
                                format!("Preset „{name}“ gespeichert")
                            };
                            toast_overlay.add_toast(libadwaita::Toast::new(&text));
                        }
                        Err(error) => {
                            toast_overlay.add_toast(libadwaita::Toast::new(&error));
                        }
                    }
                })
            };

            let window_for_response = window.clone();
            let presets_for_response = presets.clone();
            let toast_for_response = toast_overlay.clone();
            dialog.connect_response(None, move |dialog, response| {
                if response != "save" {
                    return;
                }
                dialog.close();
                let Ok(name) = validate_name(&entry.text()) else {
                    toast_for_response
                        .add_toast(libadwaita::Toast::new("Preset braucht einen Namen"));
                    return;
                };
                if presets_for_response.borrow().get(&name).is_none() {
                    write(name);
                    return;
                }
                let confirm = libadwaita::AlertDialog::builder()
                    .heading(format!("Preset „{name}“ überschreiben?"))
                    .body("Ein Preset mit diesem Namen existiert bereits.")
                    .build();
                confirm.add_response("cancel", "Abbrechen");
                confirm.add_response("overwrite", "Überschreiben");
                confirm.set_response_appearance(
                    "overwrite",
                    libadwaita::ResponseAppearance::Destructive,
                );
                confirm.set_close_response("cancel");
                let write = write.clone();
                let name_for_confirm = name.clone();
                confirm.connect_response(None, move |dialog, response| {
                    dialog.close();
                    if response == "overwrite" {
                        write(name_for_confirm.clone());
                    }
                });
                confirm.present(Some(&window_for_response));
            });
            dialog.present(Some(&window));
        })
    };

    let load_preset: Rc<dyn Fn(&str)> = {
        let panel = panel.clone();
        let presets = presets.clone();
        let toast_overlay = toast_overlay.clone();
        Rc::new(
            move |name: &str| match presets.borrow().get(name).cloned() {
                Some(preset) => {
                    panel.apply_preset(&preset);
                    toast_overlay
                        .add_toast(libadwaita::Toast::new(&format!("Preset „{name}“ geladen")));
                }
                None => {
                    toast_overlay.add_toast(libadwaita::Toast::new("Preset nicht gefunden"));
                }
            },
        )
    };

    let rename_preset: Rc<dyn Fn()> = {
        let window = window.clone();
        let presets = presets.clone();
        let persist_presets = persist_presets.clone();
        let rebuild_presets_menu = rebuild_presets_menu.clone();
        let toast_overlay = toast_overlay.clone();
        Rc::new(move || {
            let names = presets.borrow().names();
            if names.is_empty() {
                toast_overlay.add_toast(libadwaita::Toast::new("Keine Presets vorhanden"));
                return;
            }
            let selection = preset_dropdown(&names);
            let entry = gtk::Entry::builder()
                .placeholder_text("Neuer Name")
                .activates_default(true)
                .build();
            let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
            content.append(&selection);
            content.append(&entry);
            let dialog = libadwaita::AlertDialog::builder()
                .heading("Preset umbenennen")
                .extra_child(&content)
                .build();
            dialog.add_response("cancel", "Abbrechen");
            dialog.add_response("rename", "Umbenennen");
            dialog.set_response_appearance("rename", libadwaita::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("rename"));
            dialog.set_close_response("cancel");

            let presets_for_response = presets.clone();
            let persist_for_response = persist_presets.clone();
            let rebuild_for_response = rebuild_presets_menu.clone();
            let toast_for_response = toast_overlay.clone();
            let names_for_response = names.clone();
            dialog.connect_response(None, move |dialog, response| {
                if response != "rename" {
                    return;
                }
                dialog.close();
                let Some(old) = names_for_response
                    .get(selection.selected() as usize)
                    .cloned()
                else {
                    return;
                };
                let result = presets_for_response
                    .borrow_mut()
                    .rename(&old, &entry.text());
                match result {
                    Ok(true) => {
                        persist_for_response();
                        rebuild_for_response();
                        toast_for_response.add_toast(libadwaita::Toast::new(&format!(
                            "Preset „{old}“ umbenannt"
                        )));
                    }
                    Ok(false) => {
                        toast_for_response
                            .add_toast(libadwaita::Toast::new("Preset nicht gefunden"));
                    }
                    Err(error) => {
                        toast_for_response.add_toast(libadwaita::Toast::new(&error));
                    }
                }
            });
            dialog.present(Some(&window));
        })
    };

    let delete_preset: Rc<dyn Fn()> = {
        let window = window.clone();
        let presets = presets.clone();
        let persist_presets = persist_presets.clone();
        let rebuild_presets_menu = rebuild_presets_menu.clone();
        let toast_overlay = toast_overlay.clone();
        Rc::new(move || {
            let names = presets.borrow().names();
            if names.is_empty() {
                toast_overlay.add_toast(libadwaita::Toast::new("Keine Presets vorhanden"));
                return;
            }
            let selection = preset_dropdown(&names);
            let dialog = libadwaita::AlertDialog::builder()
                .heading("Preset löschen")
                .body("Gespeicherte Regelkette dauerhaft entfernen.")
                .extra_child(&selection)
                .build();
            dialog.add_response("cancel", "Abbrechen");
            dialog.add_response("delete", "Löschen");
            dialog.set_response_appearance("delete", libadwaita::ResponseAppearance::Destructive);
            dialog.set_close_response("cancel");

            let presets_for_response = presets.clone();
            let persist_for_response = persist_presets.clone();
            let rebuild_for_response = rebuild_presets_menu.clone();
            let toast_for_response = toast_overlay.clone();
            dialog.connect_response(None, move |dialog, response| {
                if response != "delete" {
                    return;
                }
                dialog.close();
                let Some(name) = names.get(selection.selected() as usize).cloned() else {
                    return;
                };
                let removed = presets_for_response.borrow_mut().remove(&name);
                if removed {
                    persist_for_response();
                    rebuild_for_response();
                    toast_for_response
                        .add_toast(libadwaita::Toast::new(&format!("Preset „{name}“ gelöscht")));
                }
            });
            dialog.present(Some(&window));
        })
    };

    let import_presets: Rc<dyn Fn()> = {
        let window = window.clone();
        let presets = presets.clone();
        let persist_presets = persist_presets.clone();
        let rebuild_presets_menu = rebuild_presets_menu.clone();
        let toast_overlay = toast_overlay.clone();
        Rc::new(move || {
            let dialog = gtk::FileDialog::builder()
                .title("Presets importieren")
                .modal(true)
                .build();
            let presets = presets.clone();
            let persist_presets = persist_presets.clone();
            let rebuild_presets_menu = rebuild_presets_menu.clone();
            let toast_overlay = toast_overlay.clone();
            dialog.open(Some(&window), gio::Cancellable::NONE, move |result| {
                let Ok(file) = result else {
                    return;
                };
                let Some(path) = file.path() else {
                    return;
                };
                let text = match std::fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(error) => {
                        toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                            "Datei nicht lesbar: {error}"
                        )));
                        return;
                    }
                };
                match Presets::from_json(&text) {
                    Ok(imported) => {
                        let stats = presets.borrow_mut().merge(imported);
                        persist_presets();
                        rebuild_presets_menu();
                        toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                            "{} Preset(s) importiert, {} ersetzt",
                            stats.imported, stats.overwritten
                        )));
                    }
                    Err(error) => {
                        toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                            "Import fehlgeschlagen: {error}"
                        )));
                    }
                }
            });
        })
    };

    let export_presets: Rc<dyn Fn()> = {
        let window = window.clone();
        let presets = presets.clone();
        let toast_overlay = toast_overlay.clone();
        Rc::new(move || {
            if presets.borrow().is_empty() {
                toast_overlay.add_toast(libadwaita::Toast::new("Keine Presets zum Exportieren"));
                return;
            }
            let text = match presets.borrow().to_json() {
                Ok(text) => text,
                Err(error) => {
                    toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                        "Export fehlgeschlagen: {error}"
                    )));
                    return;
                }
            };
            let dialog = gtk::FileDialog::builder()
                .title("Presets exportieren")
                .initial_name(PRESET_FILE)
                .modal(true)
                .build();
            let toast_overlay = toast_overlay.clone();
            dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
                let Ok(file) = result else {
                    return;
                };
                let Some(path) = file.path() else {
                    return;
                };
                match std::fs::write(&path, &text) {
                    Ok(()) => toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                        "Presets nach {} exportiert",
                        path.display()
                    ))),
                    Err(error) => {
                        toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                            "Export fehlgeschlagen: {error}"
                        )));
                    }
                }
            });
        })
    };

    let save_preset_for_button = save_current_preset.clone();
    panel
        .save_preset_button()
        .connect_clicked(move |_| save_preset_for_button());
    // Umbenennen
    let rename: Rc<dyn Fn()> = {
        let window = window.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        let toast_overlay = toast_overlay.clone();
        let journal = journal.clone();
        let journal_path = journal_path.clone();
        let sync_undo = sync_undo.clone();
        let undo_last = undo_last.clone();
        Rc::new(move || {
            let plan = state.borrow().plan.clone();
            if !state.borrow().can_rename() {
                return;
            }
            let dialog = libadwaita::AlertDialog::builder()
                .heading(format!("{} Datei(en) umbenennen?", plan.rename_rows()))
                .body(
                    "Die Vorschau ist verbindlich. Vorhandene Dateien werden nicht überschrieben.",
                )
                .build();
            dialog.add_response("cancel", "Abbrechen");
            dialog.add_response("rename", "Umbenennen");
            dialog.set_response_appearance("rename", libadwaita::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("rename"));
            dialog.set_close_response("cancel");

            let state = state.clone();
            let refresh = refresh.clone();
            let toast_overlay = toast_overlay.clone();
            let journal = journal.clone();
            let journal_path = journal_path.clone();
            let sync_undo = sync_undo.clone();
            let undo_last = undo_last.clone();
            dialog.connect_response(None, move |_, response| {
                if response != "rename" {
                    return;
                }
                let state = state.clone();
                let refresh = refresh.clone();
                let toast_overlay = toast_overlay.clone();
                let journal = journal.clone();
                let journal_path = journal_path.clone();
                let sync_undo = sync_undo.clone();
                let undo_last = undo_last.clone();
                let plan = plan.clone();
                let finish: FinishCallback = Rc::new(move |result| match result {
                    Ok(report) => {
                        apply_report(&state, &report);
                        refresh();
                        if let Some(entry) = JournalEntry::from_report(&report) {
                            let mut journal = journal.borrow_mut();
                            journal.push(entry);
                            if let Err(error) = journal.save(&journal_path) {
                                toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                                    "Journal nicht schreibbar: {error}"
                                )));
                            }
                        }
                        sync_undo();
                        let toast = libadwaita::Toast::new(&format!(
                            "{} Datei(en) umbenannt, {} unverändert",
                            report.renamed_count(),
                            report.skipped
                        ));
                        toast.set_button_label(Some("Rückgängig"));
                        let undo_for_toast = undo_last.clone();
                        toast.connect_button_clicked(move |_| undo_for_toast());
                        toast_overlay.add_toast(toast);
                    }
                    Err(error) => {
                        toast_overlay
                            .add_toast(libadwaita::Toast::new(&format!("Fehler: {error}")));
                    }
                });

                // Große Listen im Hintergrund ausführen, damit die UI bedienbar bleibt.
                if plan.rows.len() > 100 {
                    // Ergebnis im Hintergrund erzeugen und im Hauptloop abholen.
                    let (sender, receiver) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = sender.send(execute(&plan));
                    });
                    gtk::glib::timeout_add_local(Duration::from_millis(50), move || match receiver
                        .try_recv()
                    {
                        Ok(result) => {
                            finish(result);
                            gtk::glib::ControlFlow::Break
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            gtk::glib::ControlFlow::Continue
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            gtk::glib::ControlFlow::Break
                        }
                    });
                } else {
                    finish(execute(&plan));
                }
            });
            dialog.present(Some(&window));
        })
    };

    // Aktionen inkl. Tastenkürzel
    let actions = gio::SimpleActionGroup::new();
    for (name, callback) in [
        ("open", add_files.clone()),
        ("open-folder", add_folder.clone()),
        ("clear", clear_all.clone()),
        ("rename", rename.clone()),
        ("undo", undo_last.clone()),
        ("save-preset", save_current_preset.clone()),
        ("rename-preset", rename_preset.clone()),
        ("delete-preset", delete_preset.clone()),
        ("import-presets", import_presets.clone()),
        ("export-presets", export_presets.clone()),
    ] {
        let action = gio::SimpleAction::new(name, None);
        let callback = callback.clone();
        action.connect_activate(move |_, _| callback());
        actions.add_action(&action);
    }
    let load_preset_action = gio::SimpleAction::new(
        presets_menu::LOAD_ACTION_NAME,
        Some(gtk::glib::VariantTy::STRING),
    );
    {
        let load_preset = load_preset.clone();
        load_preset_action.connect_activate(move |_, parameter| {
            if let Some(name) = parameter.and_then(|value| value.str()) {
                load_preset(name);
            }
        });
    }
    actions.add_action(&load_preset_action);
    window.insert_action_group("win", Some(&actions));

    // Dateien von der Kommandozeile bzw. aus dem Dateimanager übernehmen.
    let add_from_outside: AddPaths = {
        let state = state.clone();
        let refresh = refresh.clone();
        let report_skipped = report_skipped.clone();
        Rc::new(move |paths: Vec<PathBuf>| {
            let recursive = state.borrow().recursive;
            let scan = state::expand_paths(paths, recursive);
            let mut skipped = scan.skipped;
            skipped.extend(state::add_paths(&state, scan.files));
            report_skipped.as_ref()(skipped);
            refresh();
        })
    };
    ADD_CALLBACK.with(|slot| *slot.borrow_mut() = Some(add_from_outside.clone()));
    if !initial.is_empty() {
        add_from_outside(initial);
    }

    // Drag & Drop: Dateien und Ordner auf die Liste ziehen.
    let drop_area = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY,
    );
    drop_area.connect_enter({
        let stack = stack.clone();
        move |_, _, _| {
            stack.add_css_class("drop-highlight");
            gtk::gdk::DragAction::COPY
        }
    });
    drop_area.connect_leave({
        let stack = stack.clone();
        move |_| stack.remove_css_class("drop-highlight")
    });
    drop_area.connect_drop({
        let stack = stack.clone();
        let add_from_outside = add_from_outside.clone();
        move |_, value, _, _| {
            stack.remove_css_class("drop-highlight");
            let Ok(files) = value.get::<gtk::gdk::FileList>() else {
                return false;
            };
            let paths = files
                .files()
                .into_iter()
                .filter_map(|file| file.path())
                .collect::<Vec<_>>();
            add_from_outside(paths);
            true
        }
    });
    stack.add_controller(drop_area);

    add_button.connect_clicked(move |_| add_files());
    let add_folder_for_button = add_folder.clone();
    folder_button.connect_clicked(move |_| add_folder_for_button());
    let clear_for_button = clear_all.clone();
    clear_button.connect_clicked(move |_| clear_for_button());
    let rename_for_button = rename.clone();
    rename_button.connect_clicked(move |_| rename_for_button());

    refresh();
    window.present();
}

/// Nach dem Umbenennen die Dateiliste auf die neuen Pfade umstellen.
fn apply_report(state: &SharedState, report: &ExecutionReport) {
    let mut state = state.borrow_mut();
    for (from, to) in &report.renamed {
        if let Some(index) = state.items.iter().position(|item| &item.path == from) {
            if let Ok(item) = FileItem::from_path(to.clone()) {
                state.items[index] = item;
            }
        }
    }
}

/// Auswahlliste für Preset-Namen; Reihenfolge entspricht `names`.
fn preset_dropdown(names: &[String]) -> gtk::DropDown {
    let labels = names.iter().map(String::as_str).collect::<Vec<_>>();
    gtk::DropDown::from_strings(&labels)
}

fn summary_text(state: &AppState) -> String {
    if let Some(error) = &state.chain_error {
        return format!("Regelfehler: {error}");
    }
    let plan = &state.plan;
    if state.items.is_empty() {
        return "Keine Dateien".to_string();
    }
    format!(
        "{} Datei(en) in {} Gruppe(n) · {} Umbenennung(en) · {} Problem(e)",
        state.items.len(),
        state.groups().len(),
        plan.rename_rows(),
        plan.blocking_rows()
    )
}

fn tooltip(state: &AppState) -> String {
    if state.chain_error.is_some() {
        return "Regel ist ungültig".to_string();
    }
    if !state.plan.is_executable() {
        return format!(
            "{} Zeile(n) blockieren die Ausführung",
            state.plan.blocking_rows()
        );
    }
    if state.plan.rename_rows() == 0 {
        return "Keine Änderung geplant".to_string();
    }
    String::new()
}
