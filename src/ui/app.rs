use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use gtk::gio;
use gtk::prelude::*;
use libadwaita::prelude::*;

use crate::engine::{ExecutionReport, FileItem, RenamePlan, build_plan, execute};

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

/// Pfade, die beim Start bereits übergeben wurden.
fn pending_paths() -> Vec<PathBuf> {
    std::env::args_os()
        .skip(1)
        .filter(|argument| !argument.to_string_lossy().starts_with('-'))
        .map(PathBuf::from)
        .collect()
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
        ] {
            app.set_accels_for_action(action, &accels);
        }
    });
    app.connect_activate(|app| {
        let pending = pending_paths();
        build_window(app, pending);
    });
    app.connect_open(|app, files, _hint| {
        if app.active_window().is_none() {
            build_window(app, Vec::new());
        }
        let paths = files
            .iter()
            .filter_map(|file| file.path())
            .collect::<Vec<_>>();
        if let Some(add) = add_callback() {
            add(paths);
        }
    });

    app.run();
    Ok(())
}

fn build_window(app: &libadwaita::Application, initial: Vec<PathBuf>) {
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
    let clear_button = gtk::Button::builder()
        .icon_name("edit-clear-all-symbolic")
        .tooltip_text("Alle entfernen (Ctrl+Entf)")
        .build();
    header.pack_start(&add_button);
    header.pack_start(&folder_button);
    header.pack_end(&clear_button);

    // Regelbereich
    let fire: ChangeSlot = Rc::new(RefCell::new(None));
    let fire_for_panel = fire.clone();
    let panel = Rc::new(RulePanel::new(move || {
        if let Some(callback) = fire_for_panel.borrow().as_ref() {
            callback();
        }
    }));

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
    action_bar.append(&rename_button);

    // Regeln und Liste übereinander
    let rules_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(320)
        .child(panel.widget())
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&rules_scroll);
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    content.append(&stack);

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
        .default_height(760)
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

    let add_folder: Rc<dyn Fn()> = {
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
            dialog.select_folder(
                Some(&window),
                gio::Cancellable::NONE,
                move |result| match result {
                    Ok(folder) => {
                        if let Some(path) = folder.path() {
                            let skipped = state::add_paths(&state, state::files_in_folder(&path));
                            report_skipped.as_ref()(skipped);
                            refresh();
                        }
                    }
                    Err(error) => {
                        eprintln!("Ordnerauswahl abgebrochen: {error}");
                    }
                },
            );
        })
    };

    let clear_all: Rc<dyn Fn()> = {
        let state = state.clone();
        let refresh = refresh.clone();
        Rc::new(move || {
            state.borrow_mut().items.clear();
            refresh();
        })
    };

    // Umbenennen
    let rename: Rc<dyn Fn()> = {
        let window = window.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        let toast_overlay = toast_overlay.clone();
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
            dialog.connect_response(None, move |_, response| {
                if response != "rename" {
                    return;
                }
                let state = state.clone();
                let refresh = refresh.clone();
                let toast_overlay = toast_overlay.clone();
                let plan = plan.clone();
                let finish: FinishCallback = Rc::new(move |result| match result {
                    Ok(report) => {
                        apply_report(&state, &report);
                        refresh();
                        toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                            "{} Datei(en) umbenannt, {} unverändert",
                            report.renamed_count(),
                            report.skipped
                        )));
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
    ] {
        let action = gio::SimpleAction::new(name, None);
        let callback = callback.clone();
        action.connect_activate(move |_, _| callback());
        actions.add_action(&action);
    }
    window.insert_action_group("win", Some(&actions));

    // Dateien von der Kommandozeile bzw. aus dem Dateimanager übernehmen.
    let add_from_outside: AddPaths = {
        let state = state.clone();
        let refresh = refresh.clone();
        let report_skipped = report_skipped.clone();
        Rc::new(move |paths: Vec<PathBuf>| {
            let skipped = state::add_paths(&state, paths);
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
            add_from_outside(state::paths_from_drop(&files));
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
