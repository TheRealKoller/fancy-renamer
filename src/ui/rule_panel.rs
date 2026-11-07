use std::rc::Rc;

use gtk::prelude::*;

use crate::engine::{
    CaseMode, ExifDateRule, ExifMode, MatchMode, NormalizeOptions, NormalizeRule, NumberingMode,
    NumberingRule, ReplaceRule, Rule, RuleChain,
};

type ChangeHandler = Rc<dyn Fn()>;

/// Regelbereich: vier Karten, beliebig kombinierbar und einzeln deaktivierbar.
pub struct RulePanel {
    container: gtk::Box,
    replace: ReplaceCard,
    numbering: NumberingCard,
    normalize: NormalizeCard,
    exif: ExifCard,
    error_label: gtk::Label,
}

impl RulePanel {
    pub fn new(on_change: impl Fn() + 'static) -> Self {
        let on_change: ChangeHandler = Rc::new(on_change);
        let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
        container.set_margin_top(12);
        container.set_margin_bottom(12);
        container.set_margin_start(12);
        container.set_margin_end(12);

        let replace = ReplaceCard::new(on_change.clone());
        let numbering = NumberingCard::new(on_change.clone());
        let normalize = NormalizeCard::new(on_change.clone());
        let exif = ExifCard::new(on_change.clone());

        for card in [&replace.expander, &numbering.expander, &normalize.expander, &exif.expander] {
            container.append(card);
        }

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("error");
        error_label.set_wrap(true);
        error_label.set_xalign(0.0);
        error_label.set_visible(false);
        container.append(&error_label);

        Self {
            container,
            replace,
            numbering,
            normalize,
            exif,
            error_label,
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }

    /// Regelkette aus den aktiven Karten; Fehler blockieren die Vorschau.
    pub fn build_chain(&self) -> Result<RuleChain, String> {
        let mut rules: Vec<Box<dyn Rule>> = Vec::new();
        if self.replace.enabled() {
            rules.push(Box::new(self.replace.build()?));
        }
        if self.numbering.enabled() {
            rules.push(Box::new(self.numbering.build()?));
        }
        if self.normalize.enabled() {
            rules.push(Box::new(self.normalize.build()));
        }
        if self.exif.enabled() {
            rules.push(Box::new(self.exif.build()?));
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
}

fn expander(title: &str, subtitle: &str, switch: &gtk::Switch) -> libadwaita::ExpanderRow {
    let row = libadwaita::ExpanderRow::builder()
        .title(title)
        .subtitle(subtitle)
        .expanded(false)
        .build();
    row.add_suffix(switch);
    row
}

fn entry_row(title: &str, placeholder: &str) -> (libadwaita::ActionRow, gtk::Entry) {
    let entry = gtk::Entry::builder()
        .placeholder_text(placeholder)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    let row = libadwaita::ActionRow::builder().title(title).build();
    row.add_suffix(&entry);
    (row, entry)
}

fn switch_row(title: &str) -> (libadwaita::ActionRow, gtk::Switch) {
    let switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
    let row = libadwaita::ActionRow::builder().title(title).build();
    row.add_suffix(&switch);
    row.set_activatable_widget(Some(&switch));
    (row, switch)
}

fn dropdown_row(title: &str, options: &[&str]) -> (libadwaita::ActionRow, gtk::DropDown) {
    let dropdown = gtk::DropDown::from_strings(options);
    dropdown.set_valign(gtk::Align::Center);
    let row = libadwaita::ActionRow::builder().title(title).build();
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
        let (pattern_row, pattern) = entry_row("Suchen", "z. B. IMG_ oder (\d{4})-(\d{2})");
        let (replacement_row, replacement) = entry_row("Ersetzen", "z. B. foto_ oder $3.$2.$1");
        let (ci_row, case_insensitive) = switch_row("Groß-/Kleinschreibung ignorieren");
        let (first_row, first_only) = switch_row("Nur erstes Vorkommen");

        for row in [&mode_row, &pattern_row, &replacement_row, &ci_row, &first_row] {
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

    fn build(&self) -> Result<ReplaceRule, String> {
        let mode = if self.mode.selected() == 1 {
            MatchMode::Regex
        } else {
            MatchMode::Literal
        };
        ReplaceRule::new(
            mode,
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
        let start = gtk::SpinButton::with_range(0.0, 1_000_000.0, 1.0);
        start.set_value(1.0);
        let start_row = libadwaita::ActionRow::builder().title("Startwert").build();
        start_row.add_suffix(&start);

        let step = gtk::SpinButton::with_range(1.0, 10_000.0, 1.0);
        step.set_value(1.0);
        let step_row = libadwaita::ActionRow::builder().title("Schrittweite").build();
        step_row.add_suffix(&step);

        let digits = gtk::SpinButton::with_range(1.0, 12.0, 1.0);
        digits.set_value(3.0);
        let digits_row = libadwaita::ActionRow::builder().title("Mindeststellen").build();
        digits_row.add_suffix(&digits);

        let (prefix_row, prefix) = entry_row("Präfix", "_");
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

    fn build(&self) -> Result<NumberingRule, String> {
        let mode = if self.mode.selected() == 1 {
            NumberingMode::Append
        } else {
            NumberingMode::Replace
        };
        let default_prefix = match mode {
            NumberingMode::Replace => String::new(),
            NumberingMode::Append => "_".to_string(),
        };
        // "_" ist der Standard-Trenner: im Anhänge-Modus bleibt er, im
        // Ersetzen-Modus entfällt er.
        let mut prefix = self.prefix.text().to_string();
        if prefix == "_" {
            prefix = default_prefix;
        }
        NumberingRule::new(
            mode,
            self.start.value() as u64,
            self.step.value() as u64,
            self.digits.value() as usize,
            prefix,
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
            &["unverändert", "klein", "GROSS", "Wortanfang groß", "Satzanfang groß"],
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

    fn build(&self) -> NormalizeRule {
        let case = match self.case.selected() {
            1 => CaseMode::Lower,
            2 => CaseMode::Upper,
            3 => CaseMode::Title,
            4 => CaseMode::Sentence,
            _ => CaseMode::Keep,
        };
        NormalizeRule::new(NormalizeOptions {
            case,
            spaces_to_underscore: self.spaces.is_active(),
            trim_separators: self.trim.is_active(),
            transliterate: self.transliterate.is_active(),
            strip_special: self.strip.is_active(),
        })
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

    fn build(&self) -> Result<ExifDateRule, String> {
        let mode = if self.mode.selected() == 1 {
            ExifMode::Prefix
        } else {
            ExifMode::Replace
        };
        let format = self.format.text().to_string();
        let format = if format.is_empty() {
            "%Y-%m-%d_%H-%M-%S".to_string()
        } else {
            format
        };
        ExifDateRule::new(format, mode).map_err(|error| error.to_string())
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
