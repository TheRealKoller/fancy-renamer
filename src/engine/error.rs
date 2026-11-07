use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Fehler einer einzelnen Regel, z. B. ein ungültiges Regex-Muster.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuleError {
    #[error("ungültiges Muster: {0}")]
    InvalidPattern(String),
    #[error("ungültiger Ersatztext: {0}")]
    InvalidReplacement(String),
}

/// Fehler bei der Ausführung eines Plans. Tritt hier etwas auf, wurde der
/// bereits ausgeführte Teil zurückgerollt.
#[derive(Debug, Error)]
pub enum ExecuteError {
    #[error("Plan enthält {0} blockierende Zeile(n)")]
    Blocked(usize),
    #[error("Datei hat sich seit der Vorschau geändert: {0}")]
    StalePlan(PathBuf),
    #[error("Ziel existiert bereits: {0}")]
    TargetExists(PathBuf),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{source} (Rollback ebenfalls fehlgeschlagen: {rollback})")]
    IoWithRollback {
        #[source]
        source: io::Error,
        rollback: String,
    },
}

impl ExecuteError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
