//! Konkrete Regeln. Jede Regel transformiert nur den Dateistamm.

<<<<<<< HEAD
mod normalize;
mod numbering;
mod replace;

pub use normalize::{CaseMode, NormalizeOptions, NormalizeRule};
=======
mod exif;
mod numbering;
mod replace;

pub use exif::{ExifDateRule, ExifMode};
>>>>>>> 53ac478 (feat: Regel Aufnahmedatum (EXIF) (#10))
pub use numbering::{NumberingMode, NumberingRule};
pub use replace::{MatchMode, ReplaceRule};
