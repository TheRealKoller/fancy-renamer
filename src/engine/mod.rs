//! Domänenlogik: reine Datenverarbeitung, keine GTK-Abhängigkeiten.
//!
//! Alles in diesem Modul ist ohne Display testbar.

mod error;
mod execute;
mod item;
mod plan;
mod rule;

pub use error::{ExecuteError, RuleError};
pub use execute::{ExecutionReport, execute};
pub use item::FileItem;
pub use plan::{PlanRow, PlanStatus, RenamePlan, build_plan};
pub use rule::{Rule, RuleChain, RuleContext};
