use std::path::Path;

use crate::engine::RuleError;

/// Zusatzinformationen, die eine Regel für ihre Entscheidung braucht.
#[derive(Debug, Clone, Copy)]
pub struct RuleContext<'a> {
    pub path: &'a Path,
    pub original_stem: &'a str,
    /// Position in der Dateiliste (0-basiert).
    pub index: usize,
    /// Position der Gruppe in der Liste (0-basiert).
    pub group_index: usize,
    /// Anzahl der Dateien in dieser Gruppe (mindestens 1).
    pub group_size: usize,
    /// Position der Datei innerhalb ihrer Gruppe (0-basiert).
    pub member_index: usize,
}

/// Transformiert den Dateistamm. Regeln sehen nur den Stamm und nie die
/// Erweiterung; die Erweiterung setzt die Plan-Berechnung wieder an.
pub trait Rule: std::fmt::Debug {
    fn id(&self) -> &'static str;

    fn apply(&self, stem: &str, ctx: &RuleContext<'_>) -> Result<String, RuleError>;
}

/// Geordnete Regelkette; jede Regel arbeitet auf dem Ergebnis der vorigen.
#[derive(Debug, Default)]
pub struct RuleChain {
    rules: Vec<Box<dyn Rule>>,
}

impl RuleChain {
    pub fn new(rules: Vec<Box<dyn Rule>>) -> Self {
        Self { rules }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn apply(&self, stem: &str, ctx: &RuleContext<'_>) -> Result<String, RuleError> {
        let mut current = stem.to_string();
        for rule in &self.rules {
            current = rule.apply(&current, ctx)?;
        }
        Ok(current)
    }
}
