use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use libadwaita::prelude::*;

use crate::engine::{
    CaseMode, ExifDateRule, ExifMode, MatchMode, NormalizeOptions, NormalizeRule, NumberingMode,
    NumberingRule, Preset, ReplaceRule, Rule, RuleChain, RulePreset,
};

type ChangeHandler = Rc<dyn Fn()>;

/// Regelbereich: vier Karten, beliebig kombinierbar, einzeln deaktivierbar
/// und in ihrer Reihenfolge veränderbar.
pub struct RulePanel {
    container: gtk::Box,
    hint: gtk::Label,
    cards: RefCell<Vec<CardEntry>>,
    error_label: gtk::Label,
    on_change: ChangeHandler,
    presets_button: gtk::MenuButton,
    save_preset_button: gtk::Button,
}

/// Eine Regelkarte: Widget, Aktivierung und die daraus gebaute Regel.
trait RuleCard {
    fn expander(&self) -> &libadwaita::ExpanderRow;
    fn enabled(&self) -> bool;
    fn set_enabled(&self, enabled: bool);
    fn rule_id(&self) -> &'static str;
    /// Aktuelle Parameter, unabhängig davon, ob die Karte aktiv ist.
    fn snapshot(&self) -> RulePreset;
    /// Übernimmt Parameter aus einem Preset; fremde Regelarten bleiben unberührt.
    fn apply_preset(&self, preset: &RulePreset);
    fn build(&self) -> Result<Box<dyn Rule>, String>;
}

struct CardEntry {
    card: Box<dyn RuleCard>,
    badge: gtk::Label,
    up: gtk::Button,
    down: gtk::Button,
}

impl RulePanel {
    pub fn new(on_change: impl Fn() + 'static) -> Rc<Self> {
        let on_change: ChangeHandler = Rc::new(on_change);
        let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
        container.set_margin_top(12);
        container.set_margin_bottom(12);
        container.set_margin_start(12);
        container.set_margin_end(12);
        let presets_button = gtk::MenuButton::builder()
            .label("Presets")
            .tooltip_text("Gespeicherte Regelketten laden oder verwalten")
            .build();
        let save_preset_button = gtk::Button::builder()
            .icon_name("document-save-symbolic")
            .tooltip_text("Aktuelle Regeln als Preset speichern")
            .build();
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        header.set_halign(gtk::Align::Start);
        header.append(&presets_button);
        header.append(&save_preset_button);
        container.append(&header);

        let hint = gtk::Label::new(Some(
            "Regeln werden von oben nach unten angewendet – jede Regel arbeitet auf dem \
             Ergebnis der vorherigen.",
        ));
        hint.add_css_class("dim-label");
        hint.set_wrap(true);
        hint.set_xalign(0.0);
        container.append(&hint);

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_wrap(true);
        error_label.set_xalign(0.0);
        error_label.set_visible(false);

        // Karten melden Änderungen an das Panel, das erst nach dem Bau der
        // Karten existiert; deshalb der Umweg über einen schwachen Verweis.
        let notify: Rc<RefCell<Option<ChangeHandler>>> = Rc::new(RefCell::new(None));
        let notify_for_cards = notify.clone();
        let card_change: ChangeHandler = Rc::new(move || {
            let callback = notify_for_cards.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });

        let cards: Vec<Box<dyn RuleCard>> = vec![
            Box::new(ReplaceCard::new(card_change.clone())),
            Box::new(NumberingCard::new(card_change.clone())),
            Box::new(NormalizeCard::new(card_change.clone())),
            Box::new(ExifCard::new(card_change.clone())),
        ];

        let entries = cards
            .into_iter()
            .map(|card| {
                let badge = gtk::Label::new(Some("–"));
                badge.add_css_class("numeric");
                badge.add_css_class("dim-label");
                badge.set_width_chars(2);
                badge.set_valign(gtk::Align::Center);

                let up = reorder_button("go-up-symbolic", "Regel nach oben");
                let down = reorder_button("go-down-symbolic", "Regel nach unten");
                let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                buttons.add_css_class("linked");
                buttons.append(&up);
                buttons.append(&down);

                let prefix = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                prefix.append(&badge);
                prefix.append(&buttons);
                card.expander().add_prefix(&prefix);

                CardEntry {
                    card,
                    badge,
                    up,
                    down,
                }
            })
            .collect::<Vec<_>>();

        for entry in &entries {
            container.append(entry.card.expander());
        }
        container.append(&error_label);

        let panel = Rc::new(Self {
            container,
            hint,
            cards: RefCell::new(entries),
            error_label,
            on_change,
            presets_button,
            save_preset_button,
        });
        let weak = Rc::downgrade(&panel);
        let external = panel.on_change.clone();
        *notify.borrow_mut() = Some(Rc::new(move || {
            if let Some(panel) = weak.upgrade() {
                panel.sync_ordinals();
            }
            external();
        }));

        panel.wire_reorder();
        panel.sync_buttons();
        panel.sync_ordinals();
        panel
    }
    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }

    /// Menüknopf für gespeicherte Presets; Inhalt setzt das Fenster.
    pub fn presets_button(&self) -> &gtk::MenuButton {
        &self.presets_button
    }

    /// Knopf „Aktuelle Regeln als Preset speichern“.
    pub fn save_preset_button(&self) -> &gtk::Button {
        &self.save_preset_button
    }

    /// Aktive Regeln samt Parametern in Anwendungsreihenfolge.
    pub fn snapshot(&self) -> Vec<RulePreset> {
        self.cards
            .borrow()
            .iter()
            .filter(|entry| entry.card.enabled())
            .map(|entry| entry.card.snapshot())
            .collect()
    }

    /// Ersetzt die Kette vollständig: Reihenfolge, Parameter und Auswahl
    /// kommen aus dem Preset; nicht enthaltene Regeln werden deaktiviert.
    pub fn apply_preset(&self, preset: &Preset) {
        let order = {
            let cards = self.cards.borrow();
            let mut order: Vec<usize> = Vec::new();
            for rule in &preset.rules {
                if let Some(index) = cards
                    .iter()
                    .position(|entry| entry.card.rule_id() == rule.rule_id())
                {
                    if !order.contains(&index) {
                        order.push(index);
                    }
                }
            }
            for index in 0..cards.len() {
                if !order.contains(&index) {
                    order.push(index);
                }
            }
            order
        };
        for entry in self.cards.borrow().iter() {
            let rule = preset
                .rules
                .iter()
                .find(|rule| rule.rule_id() == entry.card.rule_id());
            if let Some(rule) = rule {
                entry.card.apply_preset(rule);
            }
            entry.card.set_enabled(rule.is_some());
        }
        self.reorder(&order);
        self.sync_buttons();
        self.sync_ordinals();
        (self.on_change)();
    }

    /// Karten in die übergebene Reihenfolge bringen; fehlende Indizes bleiben
    /// in ihrer alten relativen Ordnung am Ende.
    fn reorder(&self, order: &[usize]) {
        {
            let mut cards = self.cards.borrow_mut();
            let mut slots: Vec<Option<CardEntry>> =
                std::mem::take(&mut *cards).into_iter().map(Some).collect();
            let mut sorted = Vec::with_capacity(slots.len());
            for index in order {
                if let Some(entry) = slots.get_mut(*index).and_then(Option::take) {
                    sorted.push(entry);
                }
            }
            sorted.extend(slots.into_iter().flatten());
            *cards = sorted;
        }
        let mut anchor: gtk::Widget = self.hint.clone().upcast();
        for entry in self.cards.borrow().iter() {
            self.container
                .reorder_child_after(entry.card.expander(), Some(&anchor));
            anchor = entry.card.expander().clone().upcast();
        }
    }

    /// Regelkette in der sichtbaren Reihenfolge; Fehler blockieren die Vorschau.
    pub fn build_chain(&self) -> Result<RuleChain, String> {
        let mut rules: Vec<Box<dyn Rule>> = Vec::new();
        for entry in self.cards.borrow().iter() {
            if entry.card.enabled() {
                rules.push(entry.card.build()?);
            }
        }
        Ok(RuleChain::new(rules))
    }

    pub fn show_error(&self, message: Option<&str>) {
        match message {
            Some(message) => {
                self.error_label.set_text(message);
                self.error_label.set_visible(true);
            }
            None => self.error_label.set_visible(false),
        }
    }

    fn wire_reorder(self: &Rc<Self>) {
        for entry in self.cards.borrow().iter() {
            let expander = entry.card.expander().clone();
            let panel = Rc::clone(self);
            let target = expander.clone();
            entry
                .up
                .connect_clicked(move |_| panel.move_card(&target, -1));
            let panel = Rc::clone(self);
            let target = expander.clone();
            entry
                .down
                .connect_clicked(move |_| panel.move_card(&target, 1));
        }
    }

    /// Karte um eine Position verschieben; Ränder sind No-Ops. Die Position
    /// wird über die Karte selbst gesucht, nicht über einen beim Verdrahten
    /// festgehaltenen Index.
    fn move_card(self: &Rc<Self>, expander: &libadwaita::ExpanderRow, delta: isize) {
        let mut cards = self.cards.borrow_mut();
        let Some(index) = cards
            .iter()
            .position(|entry| entry.card.expander() == expander)
        else {
            return;
        };
        let Some(target) = index.checked_add_signed(delta) else {
            return;
        };
        if target >= cards.len() {
            return;
        }
        cards.swap(index, target);
        let anchor: gtk::Widget = if target == 0 {
            self.hint.clone().upcast()
        } else {
            cards[target - 1].card.expander().clone().upcast()
        };
        self.container.reorder_child_after(expander, Some(&anchor));
        drop(cards);

        self.sync_buttons();
        self.sync_ordinals();
        (self.on_change)();
    }

    fn sync_buttons(&self) {
        let cards = self.cards.borrow();
        let last = cards.len().saturating_sub(1);
        for (index, entry) in cards.iter().enumerate() {
            entry.up.set_sensitive(index > 0);
            entry.down.set_sensitive(index < last);
        }
    }

    /// Nummeriert die aktiven Regeln in Anwendungsreihenfolge; inaktive
    /// Karten zeigen einen Strich.
    fn sync_ordinals(&self) {
        let cards = self.cards.borrow();
        let mut step = 0;
        for entry in cards.iter() {
            if entry.card.enabled() {
                step += 1;
                entry.badge.remove_css_class("dim-label");
                entry.badge.set_text(&step.to_string());
            } else {
                entry.badge.add_css_class("dim-label");
                entry.badge.set_text("–");
            }
        }
    }
}

fn reorder_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build()
}

fn expander(title: &str, subtitle: &str, switch: &gtk::Switch) -> libadwaita::ExpanderRow {
    let row = libadwaita::ExpanderRow::builder()
        .title(markup(title))
        .subtitle(markup(subtitle))
        .expanded(false)
        .build();
    row.add_suffix(switch);
    row
}

/// Zeilentitel sind Pango-Markup; Sonderzeichen müssen maskiert werden.
fn markup(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
}

fn entry_row(title: &str, placeholder: &str) -> (libadwaita::ActionRow, gtk::Entry) {
    let entry = gtk::Entry::builder()
        .placeholder_text(placeholder)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    let row = libadwaita::ActionRow::builder()
        .title(markup(title))
        .build();
    row.add_suffix(&entry);
    (row, entry)
}

fn switch_row(title: &str) -> (libadwaita::ActionRow, gtk::Switch) {
    let switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
    let row = libadwaita::ActionRow::builder()
        .title(markup(title))
        .build();
    row.add_suffix(&switch);
    row.set_activatable_widget(Some(&switch));
    (row, switch)
}

fn dropdown_row(title: &str, options: &[&str]) -> (libadwaita::ActionRow, gtk::DropDown) {
    let dropdown = gtk::DropDown::from_strings(options);
    dropdown.set_valign(gtk::Align::Center);
    let row = libadwaita::ActionRow::builder()
        .title(markup(title))
        .build();
    row.add_suffix(&dropdown);
    (row, dropdown)
}

struct ReplaceCard {
    expander: libadwaita::ExpanderRow,
    switch: gtk::Switch,
    mode: gtk::DropDown,
    pattern: gtk::Entry,
    replacement: gtk::Entry,
    case_insensitive: gtk::Switch,
    first_only: gtk::Switch,
}

impl ReplaceCard {
    fn new(on_change: ChangeHandler) -> Self {
        let switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
        let expander = expander(
            "Suchen & Ersetzen",
            "Text im Dateinamen ersetzen, wörtlich oder als Regex",
            &switch,
        );

        let (mode_row, mode) = dropdown_row("Modus", &["wörtlich", "Regex"]);
        let (pattern_row, pattern) = entry_row("Suchen", r"z. B. IMG_ oder (\d{4})-(\d{2})");
        let (replacement_row, replacement) = entry_row("Ersetzen", "z. B. foto_ oder $3.$2.$1");
        let (ci_row, case_insensitive) = switch_row("Groß-/Kleinschreibung ignorieren");
        let (first_row, first_only) = switch_row("Nur erstes Vorkommen");

        for row in [
            &mode_row,
            &pattern_row,
            &replacement_row,
            &ci_row,
            &first_row,
        ] {
            expander.add_row(row);
        }

        on_text_changed(&pattern, &on_change);
        on_text_changed(&replacement, &on_change);
        on_dropdown_changed(&mode, &on_change);
        on_switch_changed(&case_insensitive, &on_change);
        on_switch_changed(&first_only, &on_change);
        on_switch_changed(&switch, &on_change);

        Self {
            expander,
            switch,
            mode,
            pattern,
            replacement,
            case_insensitive,
            first_only,
        }
    }

    fn enabled(&self) -> bool {
        self.switch.is_active()
    }

    fn rule_id(&self) -> &'static str {
        "replace"
    }

    fn match_mode(&self) -> MatchMode {
        if self.mode.selected() == 1 {
            MatchMode::Regex
        } else {
            MatchMode::Literal
        }
    }

    fn snapshot(&self) -> RulePreset {
        RulePreset::Replace {
            mode: self.match_mode(),
            pattern: self.pattern.text().to_string(),
            replacement: self.replacement.text().to_string(),
            case_insensitive: self.case_insensitive.is_active(),
            first_only: self.first_only.is_active(),
        }
    }

    fn apply_preset(&self, preset: &RulePreset) {
        let RulePreset::Replace {
            mode,
            pattern,
            replacement,
            case_insensitive,
            first_only,
        } = preset
        else {
            return;
        };
        self.mode
            .set_selected(if *mode == MatchMode::Regex { 1 } else { 0 });
        self.pattern.set_text(pattern);
        self.replacement.set_text(replacement);
        self.case_insensitive.set_active(*case_insensitive);
        self.first_only.set_active(*first_only);
    }

    fn set_enabled(&self, enabled: bool) {
        self.switch.set_active(enabled);
    }

    fn build_rule(&self) -> Result<ReplaceRule, String> {
        ReplaceRule::new(
            self.match_mode(),
            self.pattern.text().to_string(),
            self.replacement.text().to_string(),
            self.case_insensitive.is_active(),
            self.first_only.is_active(),
        )
        .map_err(|error| error.to_string())
    }
}

struct NumberingCard {
    expander: libadwaita::ExpanderRow,
    switch: gtk::Switch,
    mode: gtk::DropDown,
    start: gtk::SpinButton,
    step: gtk::SpinButton,
    digits: gtk::SpinButton,
    prefix: gtk::Entry,
    suffix: gtk::Entry,
}

impl NumberingCard {
    fn new(on_change: ChangeHandler) -> Self {
        let switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
        let expander = expander(
            "Nummerierung",
            "Fortlaufende Nummer, bei Paaren für alle Dateien gleich",
            &switch,
        );

        let (mode_row, mode) = dropdown_row("Modus", &["Stamm ersetzen", "anhängen"]);
        // Standard ist Anhängen: so bleibt das Ergebnis vorheriger Regeln
        // (z. B. EXIF-Datum) erhalten.
        mode.set_selected(1);
        let replace_hint = gtk::Label::new(Some(
            "Stamm ersetzen verwirft das Ergebnis vorheriger Regeln.",
        ));
        replace_hint.add_css_class("dim-label");
        replace_hint.set_wrap(true);
        replace_hint.set_xalign(0.0);
        replace_hint.set_margin_top(6);
        replace_hint.set_visible(false);
        let start = gtk::SpinButton::with_range(0.0, 1_000_000.0, 1.0);
        start.set_value(1.0);
        let start_row = libadwaita::ActionRow::builder().title("Startwert").build();
        start_row.add_suffix(&start);

        let step = gtk::SpinButton::with_range(1.0, 10_000.0, 1.0);
        step.set_value(1.0);
        let step_row = libadwaita::ActionRow::builder()
            .title("Schrittweite")
            .build();
        step_row.add_suffix(&step);

        let digits = gtk::SpinButton::with_range(1.0, 12.0, 1.0);
        digits.set_value(3.0);
        let digits_row = libadwaita::ActionRow::builder()
            .title("Mindeststellen")
            .build();
        digits_row.add_suffix(&digits);

        let (prefix_row, prefix) = entry_row("Präfix", "_");
        // Standard-Trenner im Anhänge-Modus; „Stamm ersetzen“ blendet ihn aus.
        prefix.set_text("_");
        let (suffix_row, suffix) = entry_row("Suffix", "");

        for row in [
            &mode_row,
            &start_row,
            &step_row,
            &digits_row,
            &prefix_row,
            &suffix_row,
        ] {
            expander.add_row(row);
        }
        expander.add_row(&replace_hint);

        let hint_for_mode = replace_hint.clone();
        mode.connect_selected_notify(move |dropdown| {
            hint_for_mode.set_visible(dropdown.selected() == 0);
        });

        on_text_changed(&prefix, &on_change);
        on_text_changed(&suffix, &on_change);
        on_dropdown_changed(&mode, &on_change);
        for spin in [&start, &step, &digits] {
            let handler = on_change.clone();
            spin.connect_value_notify(move |_| handler());
        }
        on_switch_changed(&switch, &on_change);

        Self {
            expander,
            switch,
            mode,
            start,
            step,
            digits,
            prefix,
            suffix,
        }
    }

    fn enabled(&self) -> bool {
        self.switch.is_active()
    }

    fn rule_id(&self) -> &'static str {
        "numbering"
    }

    fn numbering_mode(&self) -> NumberingMode {
        if self.mode.selected() == 1 {
            NumberingMode::Append
        } else {
            NumberingMode::Replace
        }
    }

    /// Wirkendes Präfix: `_` steht für den Standard-Trenner beim Anhängen und
    /// entfällt beim Ersetzen.
    fn effective_prefix(&self) -> String {
        let prefix = self.prefix.text().to_string();
        if prefix != "_" {
            return prefix;
        }
        match self.numbering_mode() {
            NumberingMode::Replace => String::new(),
            NumberingMode::Append => prefix,
        }
    }

    fn snapshot(&self) -> RulePreset {
        RulePreset::Numbering {
            mode: self.numbering_mode(),
            start: self.start.value() as u64,
            step: self.step.value() as u64,
            digits: self.digits.value() as usize,
            prefix: self.effective_prefix(),
            suffix: self.suffix.text().to_string(),
        }
    }

    fn apply_preset(&self, preset: &RulePreset) {
        let RulePreset::Numbering {
            mode,
            start,
            step,
            digits,
            prefix,
            suffix,
        } = preset
        else {
            return;
        };
        self.mode.set_selected(match mode {
            NumberingMode::Replace => 0,
            NumberingMode::Append => 1,
        });
        self.start.set_value(*start as f64);
        self.step.set_value(*step as f64);
        self.digits.set_value(*digits as f64);
        self.prefix.set_text(prefix);
        self.suffix.set_text(suffix);
    }

    fn set_enabled(&self, enabled: bool) {
        self.switch.set_active(enabled);
    }

    fn build_rule(&self) -> Result<NumberingRule, String> {
        NumberingRule::new(
            self.numbering_mode(),
            self.start.value() as u64,
            self.step.value() as u64,
            self.digits.value() as usize,
            self.effective_prefix(),
            self.suffix.text().to_string(),
        )
        .map_err(|error| error.to_string())
    }
}

struct NormalizeCard {
    expander: libadwaita::ExpanderRow,
    switch: gtk::Switch,
    case: gtk::DropDown,
    spaces: gtk::Switch,
    trim: gtk::Switch,
    transliterate: gtk::Switch,
    strip: gtk::Switch,
}

impl NormalizeCard {
    fn new(on_change: ChangeHandler) -> Self {
        let switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
        let expander = expander(
            "Namen normalisieren",
            "Groß-/Kleinschreibung und Zeichen bereinigen",
            &switch,
        );

        let (case_row, case) = dropdown_row(
            "Schreibweise",
            &[
                "unverändert",
                "klein",
                "GROSS",
                "Wortanfang groß",
                "Satzanfang groß",
            ],
        );
        let (spaces_row, spaces) = switch_row("Leerzeichen durch _ ersetzen");
        let (trim_row, trim) = switch_row("Trenner am Anfang/Ende entfernen");
        let (transliterate_row, transliterate) = switch_row("Umlaute transliterieren (ä → ae)");
        let (strip_row, strip) = switch_row("Sonderzeichen entfernen");

        for row in [
            &case_row,
            &spaces_row,
            &trim_row,
            &transliterate_row,
            &strip_row,
        ] {
            expander.add_row(row);
        }

        on_dropdown_changed(&case, &on_change);
        for widget in [&spaces, &trim, &transliterate, &strip, &switch] {
            on_switch_changed(widget, &on_change);
        }

        Self {
            expander,
            switch,
            case,
            spaces,
            trim,
            transliterate,
            strip,
        }
    }

    fn enabled(&self) -> bool {
        self.switch.is_active()
    }

    fn rule_id(&self) -> &'static str {
        "normalize"
    }

    fn case_mode(&self) -> CaseMode {
        match self.case.selected() {
            1 => CaseMode::Lower,
            2 => CaseMode::Upper,
            3 => CaseMode::Title,
            4 => CaseMode::Sentence,
            _ => CaseMode::Keep,
        }
    }

    fn snapshot(&self) -> RulePreset {
        RulePreset::Normalize {
            case: self.case_mode(),
            spaces_to_underscore: self.spaces.is_active(),
            trim_separators: self.trim.is_active(),
            transliterate: self.transliterate.is_active(),
            strip_special: self.strip.is_active(),
        }
    }

    fn apply_preset(&self, preset: &RulePreset) {
        let RulePreset::Normalize {
            case,
            spaces_to_underscore,
            trim_separators,
            transliterate,
            strip_special,
        } = preset
        else {
            return;
        };
        self.case.set_selected(match case {
            CaseMode::Keep => 0,
            CaseMode::Lower => 1,
            CaseMode::Upper => 2,
            CaseMode::Title => 3,
            CaseMode::Sentence => 4,
        });
        self.spaces.set_active(*spaces_to_underscore);
        self.trim.set_active(*trim_separators);
        self.transliterate.set_active(*transliterate);
        self.strip.set_active(*strip_special);
    }

    fn set_enabled(&self, enabled: bool) {
        self.switch.set_active(enabled);
    }

    fn build_rule(&self) -> Result<NormalizeRule, String> {
        Ok(NormalizeRule::new(NormalizeOptions {
            case: self.case_mode(),
            spaces_to_underscore: self.spaces.is_active(),
            trim_separators: self.trim.is_active(),
            transliterate: self.transliterate.is_active(),
            strip_special: self.strip.is_active(),
        }))
    }
}

struct ExifCard {
    expander: libadwaita::ExpanderRow,
    switch: gtk::Switch,
    mode: gtk::DropDown,
    format: gtk::Entry,
}

impl ExifCard {
    fn new(on_change: ChangeHandler) -> Self {
        let switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
        let expander = expander(
            "Aufnahmedatum (EXIF)",
            "Dateiname aus dem EXIF-Datum von Fotos",
            &switch,
        );

        let (mode_row, mode) = dropdown_row("Modus", &["Stamm ersetzen", "voranstellen"]);
        let (format_row, format) = entry_row("Datumsformat", "%Y-%m-%d_%H-%M-%S");

        expander.add_row(&mode_row);
        expander.add_row(&format_row);

        on_text_changed(&format, &on_change);
        on_dropdown_changed(&mode, &on_change);
        on_switch_changed(&switch, &on_change);

        Self {
            expander,
            switch,
            mode,
            format,
        }
    }

    fn enabled(&self) -> bool {
        self.switch.is_active()
    }

    fn rule_id(&self) -> &'static str {
        "exif_date"
    }

    fn exif_mode(&self) -> ExifMode {
        if self.mode.selected() == 1 {
            ExifMode::Prefix
        } else {
            ExifMode::Replace
        }
    }

    fn effective_format(&self) -> String {
        let format = self.format.text().to_string();
        if format.is_empty() {
            "%Y-%m-%d_%H-%M-%S".to_string()
        } else {
            format
        }
    }

    fn snapshot(&self) -> RulePreset {
        RulePreset::ExifDate {
            format: self.effective_format(),
            mode: self.exif_mode(),
        }
    }

    fn apply_preset(&self, preset: &RulePreset) {
        let RulePreset::ExifDate { format, mode } = preset else {
            return;
        };
        self.mode.set_selected(match mode {
            ExifMode::Replace => 0,
            ExifMode::Prefix => 1,
        });
        self.format.set_text(format);
    }

    fn set_enabled(&self, enabled: bool) {
        self.switch.set_active(enabled);
    }

    fn build_rule(&self) -> Result<ExifDateRule, String> {
        ExifDateRule::new(self.effective_format(), self.exif_mode())
            .map_err(|error| error.to_string())
    }
}

fn on_text_changed(entry: &gtk::Entry, handler: &ChangeHandler) {
    let handler = handler.clone();
    entry.connect_changed(move |_| handler());
}

fn on_dropdown_changed(dropdown: &gtk::DropDown, handler: &ChangeHandler) {
    let handler = handler.clone();
    dropdown.connect_notify_local(Some("selected"), move |_, _| handler());
}

fn on_switch_changed(switch: &gtk::Switch, handler: &ChangeHandler) {
    let handler = handler.clone();
    switch.connect_active_notify(move |_| handler());
}

macro_rules! impl_rule_card {
    ($card:ty, $rule:ty) => {
        impl RuleCard for $card {
            fn expander(&self) -> &libadwaita::ExpanderRow {
                &self.expander
            }

            fn enabled(&self) -> bool {
                <$card>::enabled(self)
            }

            fn set_enabled(&self, enabled: bool) {
                <$card>::set_enabled(self, enabled)
            }

            fn rule_id(&self) -> &'static str {
                <$card>::rule_id(self)
            }

            fn snapshot(&self) -> RulePreset {
                <$card>::snapshot(self)
            }

            fn apply_preset(&self, preset: &RulePreset) {
                <$card>::apply_preset(self, preset)
            }

            fn build(&self) -> Result<Box<dyn Rule>, String> {
                <$card>::build_rule(self).map(|rule| Box::new(rule) as Box<dyn Rule>)
            }
        }
    };
}

impl_rule_card!(ReplaceCard, ReplaceRule);
impl_rule_card!(NumberingCard, NumberingRule);
impl_rule_card!(NormalizeCard, NormalizeRule);
impl_rule_card!(ExifCard, ExifDateRule);
