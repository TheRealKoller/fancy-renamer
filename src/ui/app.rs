use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use gtk::gio;
use gtk::prelude::*;

use crate::engine::{FileItem, ExecutionReport, execute};

use super::rule_panel::RulePanel;
use super::state::{AppState, SharedState};
use super::{file_list, state};

const APP_ID: &str = "io.github.therealKoller.FancyRenamer";

pub fn run() -> Result<()> {
    let app = libadwaita::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_startup(|app| {
        for (action, accels) in [
            ("win.open", vec!["<Control>o"]),
            ("win.rename", vec!["<Control>Return"]),
            ("win.clear", vec!["<Control>Delete"]),
        ] {
            app.set_accels_for_action(action, &accels);
        }
    });
    app.connect_activate(build_window);

    app.run();
    Ok(())
}

fn build_window(app: &libadwaita::Application) {
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

    let window = libadwaita::ApplicationWindow::builder()
        .application(app)
        .title("fancy-renamer")
        .default_width(1100)
        .default_height(760)
        .build();
    window.set_titlebar(Some(&header));

    // Regelbereich
    let fire: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));
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

    let toolbar_view = libadwaita::ToolbarView::builder()
        .content(&content)
        .build();
    toolbar_view.add_bottom_bar(&action_bar);

    let toast_overlay = libadwaita::ToastOverlay::new();
    toast_overlay.set_child(Some(&toolbar_view));
    window.set_content(Some(&toast_overlay));

    // Vorschau neu berechnen (entprellt, damit Tippen nicht jedes Zeichen rechnet)
    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let panel = panel.clone();
        let list = list.clone();
        let stack = stack.clone();
        let summary = summary.clone();
        let rename_button = rename_button.clone();
        Rc::new(move || {
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
            state.refresh();

            file_list::render(&list, &state.plan);
            panel.show_error(state.chain_error.as_deref());
            summary.set_text(&summary_text(&state));
            rename_button.set_sensitive(state.can_rename());
            rename_button.set_tooltip_text(Some(&tooltip(&state)));
            stack.set_visible_child_name(if state.items.is_empty() { "empty" } else { "list" });
        })
    };

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
    let add_files = {
        let window = window.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        Rc::new(move || {
            let dialog = gtk::FileDialog::builder()
                .title("Dateien auswählen")
                .modal(true)
                .build();
            let state = state.clone();
            let refresh = refresh.clone();
            dialog.open_multiple(
                Some(&window),
                gio::Cancellable::NONE,
                move |result| match result {
                    Ok(files) => {
                        let paths = (0..files.n_items()).filter_map(|index| {
                            files
                                .item(index)
                                .and_then(|object| object.downcast::<gio::File>().ok())
                                .and_then(|file| file.path())
                        });
                        state::add_paths(&state, paths);
                        refresh();
                    }
                    Err(error) => {
                        eprintln!("Dateiauswahl abgebrochen: {error}");
                    }
                },
            );
        })
    };

    let add_folder = {
        let window = window.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        Rc::new(move || {
            let dialog = gtk::FileDialog::builder()
                .title("Ordner auswählen")
                .modal(true)
                .build();
            let state = state.clone();
            let refresh = refresh.clone();
            dialog.select_folder(
                Some(&window),
                gio::Cancellable::NONE,
                move |result| match result {
                    Ok(folder) => {
                        if let Some(path) = folder.path() {
                            state::add_paths(&state, state::files_in_folder(&path));
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

    let clear_all = {
        let state = state.clone();
        let refresh = refresh.clone();
        Rc::new(move || {
            state.borrow_mut().items.clear();
            refresh();
        })
    };

    // Umbenennen
    let rename = {
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
                .body("Die Vorschau ist verbindlich. Vorhandene Dateien werden nicht überschrieben.")
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
                let finish = move |result: Result<ExecutionReport, crate::engine::ExecuteError>| {
                    match result {
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
                            toast_overlay.add_toast(libadwaita::Toast::new(&format!(
                                "Fehler: {error}"
                            )));
                        }
                    }
                };

                // Große Listen im Hintergrund ausführen, damit die UI bedienbar bleibt.
                if plan.rows.len() > 100 {
                    std::thread::spawn(move || {
                        let result = execute(&plan);
                        let finish = finish.clone();
                        gtk::glib::idle_add_once(move || finish(result));
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
        "{} Datei(en) · {}/{} Gruppen · {} Umbenennung(en) · {} Problem(e)",
        state.items.len(),
        state.groups().len(),
        state.items.len(),
        plan.rename_rows(),
        plan.blocking_rows()
    )
}

fn tooltip(state: &AppState) -> String {
    if state.chain_error.is_some() {
        return "Regel ist ungültig".to_string();
    }
    if !state.plan.is_executable() {
        return format!("{} Zeile(n) blockieren die Ausführung", state.plan.blocking_rows());
    }
    if state.plan.rename_rows() == 0 {
        return "Keine Änderung geplant".to_string();
    }
    String::new()
}

/// Sichtbarer Dateiname inklusive Erweiterung.
pub fn display_name(item: &FileItem) -> String {
    item.file_name_for(&item.stem)
}

#[allow(dead_code)]
fn paths(items: &[FileItem]) -> Vec<PathBuf> {
    items.iter().map(|item| item.path.clone()).collect()
}
