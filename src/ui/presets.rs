//! Aufbau des Preset-Menüs im Regelbereich.
//!
//! Die Einträge laden ein Preset, die Verwaltung öffnet Dialoge für
//! Umbenennen, Löschen sowie Import und Export.

use gtk::gio;
use gtk::prelude::*;

use crate::engine::Presets;

/// Action-Name innerhalb der Fenstergruppe.
pub const LOAD_ACTION_NAME: &str = "load-preset";

/// Action zum Laden eines Presets; das Ziel ist der Preset-Name.
pub const LOAD_ACTION: &str = "win.load-preset";

/// Namen der Presets in Menüreihenfolge.
pub fn entry_labels(presets: &Presets) -> Vec<String> {
    presets.names()
}

/// Menü mit einem Eintrag je Preset und einem Abschnitt zur Verwaltung.
pub fn menu(presets: &Presets) -> gio::Menu {
    let menu = gio::Menu::new();
    let entries = gio::Menu::new();
    let labels = entry_labels(presets);
    if labels.is_empty() {
        entries.append(Some("Noch keine Presets"), None);
    } else {
        for name in labels {
            let item = gio::MenuItem::new(Some(&name), Some(LOAD_ACTION));
            item.set_attribute_value("target", Some(&name.to_variant()));
            entries.append_item(&item);
        }
    }
    menu.append_section(None, &entries);

    let manage = gio::Menu::new();
    manage.append(Some("Preset umbenennen…"), Some("win.rename-preset"));
    manage.append(Some("Preset löschen…"), Some("win.delete-preset"));
    manage.append(Some("Presets importieren…"), Some("win.import-presets"));
    manage.append(Some("Presets exportieren…"), Some("win.export-presets"));
    menu.append_section(None, &manage);
    menu
}
