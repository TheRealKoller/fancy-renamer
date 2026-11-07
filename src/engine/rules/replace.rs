use regex::{Regex, RegexBuilder};

use crate::engine::{Rule, RuleContext, RuleError};

/// Wörtliche Ersetzung oder regulärer Ausdruck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    Literal,
    Regex,
}

/// Ersetzt Text im Stamm – wörtlich oder per Regex (mit `$1`-Captures).
#[derive(Debug)]
pub struct ReplaceRule {
    /// Immer ein Regex; bei `Literal` wird das Muster escaped und der Ersatz
    /// nicht als Template interpretiert. `None` nur bei leerem Muster.
    compiled: Option<Regex>,
    pattern: String,
    replacement: Replacement,
    first_only: bool,
}

#[derive(Debug)]
enum Replacement {
    /// Wird wörtlich eingesetzt, `$` hat keine Bedeutung.
    Literal(String),
    /// Wird als Regex-Template interpretiert (`$1`, `${name}`).
    Template(String),
}

impl ReplaceRule {
    /// `Literal`: `pattern` wird als Text gesucht, `replacement` wörtlich eingesetzt.
    /// `Regex`: `pattern` ist ein regulärer Ausdruck; ein ungültiges Muster ist ein Fehler.
    pub fn new(
        mode: MatchMode,
        pattern: impl Into<String>,
        replacement: impl Into<String>,
        case_insensitive: bool,
        first_only: bool,
    ) -> Result<Self, RuleError> {
        let pattern = pattern.into();
        let replacement = replacement.into();
        if mode == MatchMode::Literal && pattern.is_empty() {
            return Ok(Self {
                compiled: None,
                pattern,
                replacement: Replacement::Literal(replacement),
                first_only,
            });
        }
        let expression = match mode {
            MatchMode::Literal => regex::escape(&pattern),
            MatchMode::Regex => pattern.clone(),
        };
        let compiled = RegexBuilder::new(&expression)
            .case_insensitive(case_insensitive)
            .build()
            .map_err(|error| RuleError::InvalidPattern(error.to_string()))?;
        let replacement = match mode {
            MatchMode::Literal => Replacement::Literal(replacement),
            MatchMode::Regex => Replacement::Template(replacement),
        };
        Ok(Self {
            compiled: Some(compiled),
            pattern,
            replacement,
            first_only,
        })
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn replacement(&self) -> &str {
        match &self.replacement {
            Replacement::Literal(text) | Replacement::Template(text) => text,
        }
    }

    pub fn mode(&self) -> MatchMode {
        if matches!(self.replacement, Replacement::Template(_)) {
            MatchMode::Regex
        } else {
            MatchMode::Literal
        }
    }
}

impl Rule for ReplaceRule {
    fn id(&self) -> &'static str {
        "replace"
    }

    fn apply(&self, stem: &str, _ctx: &RuleContext<'_>) -> Result<String, RuleError> {
        let Some(regex) = &self.compiled else {
            return Ok(stem.to_string());
        };
        Ok(match (&self.replacement, self.first_only) {
            (Replacement::Literal(text), true) => {
                regex.replacen(stem, 1, regex::NoExpand(text)).into_owned()
            }
            (Replacement::Literal(text), false) => {
                regex.replace_all(stem, regex::NoExpand(text)).into_owned()
            }
            (Replacement::Template(text), true) => {
                regex.replacen(stem, 1, text.as_str()).into_owned()
            }
            (Replacement::Template(text), false) => {
                regex.replace_all(stem, text.as_str()).into_owned()
            }
        })
    }
}
