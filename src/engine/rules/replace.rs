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
    mode: MatchMode,
    /// Bei `Literal` das Muster, bei `Regex` der Ausdruck.
    pattern: String,
    /// Bei `Literal` der Ersatztext, bei `Regex` der Ersatztext mit Capture-Referenzen.
    replacement: String,
    /// Nur bei `Regex` gebraucht; bei `Literal` über `replace`/`replacen` erledigt.
    compiled: Option<Regex>,
    first_only: bool,
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
        let compiled = match mode {
            MatchMode::Literal => None,
            MatchMode::Regex => {
                let regex = RegexBuilder::new(&pattern)
                    .case_insensitive(case_insensitive)
                    .build()
                    .map_err(|error| RuleError::InvalidPattern(error.to_string()))?;
                Some(regex)
            }
        };
        Ok(Self {
            mode,
            pattern,
            replacement: replacement.into(),
            compiled,
            first_only,
        })
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    pub fn mode(&self) -> MatchMode {
        self.mode
    }
}

impl Rule for ReplaceRule {
    fn id(&self) -> &'static str {
        "replace"
    }

    fn apply(&self, stem: &str, _ctx: &RuleContext<'_>) -> Result<String, RuleError> {
        match self.mode {
            MatchMode::Literal => {
                if self.pattern.is_empty() {
                    return Ok(stem.to_string());
                }
                Ok(if self.first_only {
                    stem.replacen(&self.pattern, &self.replacement, 1)
                } else {
                    stem.replace(&self.pattern, &self.replacement)
                })
            }
            MatchMode::Regex => {
                let compiled = self
                    .compiled
                    .as_ref()
                    .ok_or_else(|| RuleError::InvalidPattern("Muster fehlt".into()))?;
                Ok(if self.first_only {
                    compiled
                        .replace(stem, self.replacement.as_str())
                        .into_owned()
                } else {
                    compiled
                        .replace_all(stem, self.replacement.as_str())
                        .into_owned()
                })
            }
        }
    }
}
