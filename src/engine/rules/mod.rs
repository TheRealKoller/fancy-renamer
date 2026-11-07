//! Konkrete Regeln. Jede Regel transformiert nur den Dateistamm.

mod numbering;
mod replace;

pub use numbering::{NumberingMode, NumberingRule};
pub use replace::{MatchMode, ReplaceRule};
