use crate::engine::{Rule, RuleContext, RuleError, RuleOutcome};

/// Gewünschte Groß-/Kleinschreibung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseMode {
    #[default]
    Keep,
    Lower,
    Upper,
    /// Erstes Zeichen jedes Worts groß.
    Title,
    /// Nur der Satzanfang groß, alles andere klein.
    Sentence,
}

/// Schritte der Zeichenbereinigung, unabhängig kombinierbar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NormalizeOptions {
    pub case: CaseMode,
    /// Leerzeichen und Unterstriche zu genau einem `_` zusammenfassen.
    pub spaces_to_underscore: bool,
    /// Führende und abschließende `_`/Leerzeichen entfernen.
    pub trim_separators: bool,
    /// Umlaute und ß transliterieren (`ä` → `ae`, `ß` → `ss`).
    pub transliterate: bool,
    /// Alles außer ASCII-Alphanumerik, `.`, `_`, `-` und Nicht-ASCII-Buchstaben entfernen.
    pub strip_special: bool,
}

impl NormalizeOptions {
    pub fn is_noop(&self) -> bool {
        *self == Self::default()
    }
}

/// Normalisiert den Stamm; die Reihenfolge der Schritte ist fest, damit das
/// Ergebnis reproduzierbar und idempotent ist.
#[derive(Debug, Clone, Copy, Default)]
pub struct NormalizeRule {
    options: NormalizeOptions,
}

impl NormalizeRule {
    pub fn new(options: NormalizeOptions) -> Self {
        Self { options }
    }

    pub fn options(&self) -> NormalizeOptions {
        self.options
    }
}

impl Rule for NormalizeRule {
    fn id(&self) -> &'static str {
        "normalize"
    }

    fn apply(&self, stem: &str, _ctx: &RuleContext<'_>) -> Result<RuleOutcome, RuleError> {
        let mut name = stem.to_string();
        if self.options.transliterate {
            name = transliterate(&name);
        }
        if self.options.strip_special {
            name = strip_special(&name);
        }
        if self.options.spaces_to_underscore {
            name = collapse_separators(&name);
        }
        if self.options.trim_separators {
            name = name
                .trim_matches(|c: char| c == '_' || c.is_whitespace())
                .to_string();
        }
        Ok(RuleOutcome::Stem(apply_case(&name, self.options.case)))
    }
}

/// Leerzeichen und `_` zu genau einem `_`; bestehende `__` werden zusammengefasst.
fn collapse_separators(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut previous_separator = false;
    for character in name.chars() {
        let is_separator = character == '_' || character.is_whitespace();
        if is_separator {
            if !previous_separator {
                result.push('_');
            }
        } else {
            result.push(character);
        }
        previous_separator = is_separator;
    }
    result
}

fn transliterate(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    for character in name.chars() {
        match character {
            'ä' => result.push_str("ae"),
            'ö' => result.push_str("oe"),
            'ü' => result.push_str("ue"),
            'Ä' => result.push_str("Ae"),
            'Ö' => result.push_str("Oe"),
            'Ü' => result.push_str("Ue"),
            'ß' => result.push_str("ss"),
            other => result.push(other),
        }
    }
    result
}

fn strip_special(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') || !c.is_ascii())
        .collect()
}

fn apply_case(name: &str, mode: CaseMode) -> String {
    match mode {
        CaseMode::Keep => name.to_string(),
        CaseMode::Lower => name.to_lowercase(),
        CaseMode::Upper => name.to_uppercase(),
        CaseMode::Title => capitalize_words(name),
        CaseMode::Sentence => capitalize_sentences(name),
    }
}

/// Erstes Zeichen jedes Worts groß, Rest klein; Wörter beginnen nach
/// Leerraum, `_` und `-`.
fn capitalize_words(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut at_word_start = true;
    for character in name.chars() {
        if character.is_alphanumeric() {
            if at_word_start {
                result.extend(character.to_uppercase());
            } else {
                result.extend(character.to_lowercase());
            }
            at_word_start = false;
        } else {
            result.push(character);
            if character.is_whitespace() || character == '_' || character == '-' {
                at_word_start = true;
            }
        }
    }
    result
}

/// Alles klein, erstes alphabetisches Zeichen groß.
fn capitalize_sentences(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut result = String::with_capacity(lower.len());
    let mut first = true;
    for character in lower.chars() {
        if first && character.is_alphabetic() {
            result.extend(character.to_uppercase());
            first = false;
        } else {
            result.push(character);
        }
    }
    result
}
