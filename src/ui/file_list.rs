use gtk::prelude::*;

use crate::engine::{PlanStatus, RenamePlan};

/// Baut die Vorschau-Liste: pro Gruppe eine Kopfzeile (nur bei Paaren) und je
/// Datei eine Zeile mit altem Namen, Zielnamen und Status.
pub fn render(list: &gtk::ListBox, plan: &RenamePlan) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let mut index = 0;
    while index < plan.rows.len() {
        let start = index;
        let mut end = index + 1;
        while end < plan.rows.len()
            && plan.rows[end].new_stem == plan.rows[start].new_stem
            && plan.rows[end].item.stem == plan.rows[start].item.stem
        {
            end += 1;
        }
        let group_rows = &plan.rows[start..end];
        if group_rows.len() > 1 {
            list.append(&group_header(plan, start));
        }
        for row in group_rows {
            list.append(&file_row(row, group_rows.len() > 1));
        }
        index = end;
    }
}

fn group_header(plan: &RenamePlan, start: usize) -> gtk::ListBoxRow {
    let row = plan.rows.get(start);
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header.add_css_class("heading");

    let label = gtk::Label::new(None);
    if let Some(row) = row {
        let target = if row.status.is_rename() {
            row.new_stem.as_str()
        } else {
            row.item.stem.as_str()
        };
        label.set_markup(&format!(
            "Gruppe <b>{}</b> → <b>{}</b>",
            escape(&row.item.stem),
            escape(target)
        ));
    }
    header.append(&label);

    let item_row = gtk::ListBoxRow::new();
    item_row.set_activatable(false);
    item_row.set_selectable(false);
    item_row.set_child(Some(&header));
    item_row.add_css_class("dim-label");
    item_row
}

fn file_row(row: &crate::engine::PlanRow, indented: bool) -> gtk::ListBoxRow {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    line.set_margin_top(6);
    line.set_margin_bottom(6);
    if indented {
        line.set_margin_start(24);
    }

    let old_name = gtk::Label::new(Some(&file_name(&row.item)));
    old_name.set_xalign(0.0);
    old_name.set_hexpand(true);
    old_name.add_css_class("dim-label");
    old_name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    old_name.set_tooltip_text(Some(&row.item.path.display().to_string()));

    let arrow = gtk::Label::new(Some("→"));
    arrow.add_css_class("dim-label");

    let new_name = gtk::Label::new(Some(&row.item.file_name_for(&row.new_stem)));
    new_name.set_xalign(0.0);
    new_name.set_hexpand(true);
    new_name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    if !row.status.is_rename() {
        new_name.add_css_class("dim-label");
    }

    let status = gtk::Label::new(Some(row.status.label()));
    status.add_css_class("caption");
    status.add_css_class(match row.status {
        PlanStatus::Ok | PlanStatus::CaseOnly => "success",
        PlanStatus::Collision
        | PlanStatus::Duplicate
        | PlanStatus::InvalidName
        | PlanStatus::RuleError => "error",
        PlanStatus::Unchanged | PlanStatus::Skipped => "dim-label",
    });
    if let Some(note) = &row.note {
        status.set_tooltip_text(Some(note));
    }
    if let Some(error) = &row.rule_error {
        status.set_tooltip_text(Some(error));
        status.set_text("Regelfehler");
    }

    line.append(&old_name);
    line.append(&arrow);
    line.append(&new_name);
    line.append(&status);

    let item_row = gtk::ListBoxRow::new();
    item_row.set_activatable(false);
    item_row.set_selectable(false);
    item_row.set_child(Some(&line));
    item_row
}

/// Dateinamen für Markup entschärfen.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Dateiname inklusive Erweiterung, wie er in der Liste steht.
fn file_name(item: &crate::engine::FileItem) -> String {
    item.file_name_for(&item.stem)
}
