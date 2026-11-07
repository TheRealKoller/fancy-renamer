//! Konkrete Regeln. Jede Regel transformiert nur den Dateistamm.

mod exif;
mod normalize;
mod numbering;
mod replace;

pub use exif::{ExifDateRule, ExifMode};
pub use normalize::{CaseMode, NormalizeOptions, NormalizeRule};
pub use numbering::{NumberingMode, NumberingRule};
pub use replace::{MatchMode, ReplaceRule};
