use serde::{Deserialize, Serialize};

use crate::engine::{Rule, RuleContext, RuleError, RuleOutcome};

/// Ersetzt den Stamm durch die Nummer oder hängt sie an.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumberingMode {
    Replace,
    Append,
}

/// Fortlaufende Nummerierung. Die Nummer zählt pro Gruppe: alle Dateien eines
/// Paares bekommen dieselbe Nummer.
#[derive(Debug, Clone)]
pub struct NumberingRule {
    start: u64,
    step: u64,
    digits: usize,
    prefix: String,
    suffix: String,
    mode: NumberingMode,
}

impl NumberingRule {
    pub fn new(
        mode: NumberingMode,
        start: u64,
        step: u64,
        digits: usize,
        prefix: impl Into<String>,
        suffix: impl Into<String>,
    ) -> Result<Self, RuleError> {
        if step == 0 {
            return Err(RuleError::InvalidReplacement(
                "Schrittweite darf nicht 0 sein".into(),
            ));
        }
        Ok(Self {
            start,
            step,
            digits,
            prefix: prefix.into(),
            suffix: suffix.into(),
            mode,
        })
    }

    /// Sinnvolle Vorgaben: Start 1, Schritt 1, drei Stellen. Im Anhänge-Modus
    /// trennt ein Unterstrich die Nummer vom bestehenden Stamm.
    pub fn with_defaults(mode: NumberingMode) -> Self {
        let prefix = match mode {
            NumberingMode::Replace => "",
            NumberingMode::Append => "_",
        };
        Self {
            start: 1,
            step: 1,
            digits: 3,
            prefix: prefix.to_string(),
            suffix: String::new(),
            mode,
        }
    }

    pub fn mode(&self) -> NumberingMode {
        self.mode
    }

    /// Nummer für eine Gruppe (0-basiert); mindestens so breit wie `digits`.
    fn number_for(&self, group_index: usize) -> String {
        let value = self.start + self.step * group_index as u64;
        let text = value.to_string();
        let zeros = self.digits.saturating_sub(text.len());
        format!("{}{text}", "0".repeat(zeros))
    }
}

impl Rule for NumberingRule {
    fn id(&self) -> &'static str {
        "numbering"
    }

    fn apply(&self, stem: &str, ctx: &RuleContext<'_>) -> Result<RuleOutcome, RuleError> {
        let number = self.number_for(ctx.group_index);
        let inserted = format!("{}{number}{}", self.prefix, self.suffix);
        Ok(RuleOutcome::Stem(match self.mode {
            NumberingMode::Replace => inserted,
            NumberingMode::Append => format!("{stem}{inserted}"),
        }))
    }
}
