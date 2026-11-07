use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::{ExecuteError, RenamePlan};

/// Ergebnis einer erfolgreichen Ausführung.
#[derive(Debug, Default, Clone)]
pub struct ExecutionReport {
    pub renamed: Vec<(PathBuf, PathBuf)>,
    pub skipped: usize,
}

impl ExecutionReport {
    pub fn renamed_count(&self) -> usize {
        self.renamed.len()
    }
}

/// Ein vorbereiteter Umbenennungsschritt: Original, temporärer Name, Ziel.
struct Step {
    original: PathBuf,
    temporary: PathBuf,
    target: PathBuf,
    moved_to_temporary: bool,
}

/// Führt den Plan aus: erst alle Quellen auf temporäre Namen, dann auf die
/// Zielnamen. So sind Namenszyklen möglich, und es wird nie eine Datei
/// überschrieben. Bei einem Fehler wird der bereits ausgeführte Teil
/// zurückgerollt.
pub fn execute(plan: &RenamePlan) -> Result<ExecutionReport, ExecuteError> {
    if !plan.is_executable() {
        return Err(ExecuteError::Blocked(plan.blocking_rows()));
    }

    let mut steps: Vec<Step> = plan
        .rows
        .iter()
        .filter(|row| row.status.is_rename())
        .enumerate()
        .map(|(index, row)| Step {
            original: row.item.path.clone(),
            temporary: temporary_path(&row.item.path, index),
            target: row.new_path.clone(),
            moved_to_temporary: false,
        })
        .collect();

    // Dateien dürfen sich seit der Vorschau nicht verändert haben.
    for row in plan.rows.iter().filter(|row| row.status.is_rename()) {
        if !row.item.is_unchanged_on_disk() {
            return Err(ExecuteError::StalePlan(row.item.path.clone()));
        }
    }

    // Phase 1: Quellen auf temporäre Namen.
    for step in &mut steps {
        if let Err(error) = fs::rename(&step.original, &step.temporary) {
            let failure = ExecuteError::io(&step.original, error);
            return Err(with_rollback(failure, &steps));
        }
        step.moved_to_temporary = true;
    }

    // Phase 2: temporäre Namen auf die Ziele.
    let mut report = ExecutionReport {
        skipped: plan.rows.len() - steps.len(),
        ..Default::default()
    };
    for step in &mut steps {
        if step.target.exists() {
            let failure = ExecuteError::TargetExists(step.target.clone());
            return Err(with_rollback(failure, &steps));
        }
        if let Err(error) = fs::rename(&step.temporary, &step.target) {
            let failure = ExecuteError::io(&step.target, error);
            return Err(with_rollback(failure, &steps));
        }
        step.moved_to_temporary = false;
        report
            .renamed
            .push((step.original.clone(), step.target.clone()));
    }

    Ok(report)
}

/// Rollt alle schon ausgeführten Schritte zurück und liefert den ursprünglichen
/// Fehler – angereichert, falls auch der Rollback scheitert.
fn with_rollback(failure: ExecuteError, steps: &[Step]) -> ExecuteError {
    match rollback(steps) {
        Ok(()) => failure,
        Err(rollback_error) => ExecuteError::IoWithRollback {
            source: std::io::Error::other(failure.to_string()),
            rollback: rollback_error,
        },
    }
}

/// Stellt den Zustand vor der Ausführung wieder her, soweit möglich.
fn rollback(steps: &[Step]) -> Result<(), String> {
    let mut errors = Vec::new();
    for step in steps.iter().rev() {
        let result = if step.temporary.exists() {
            fs::rename(&step.temporary, &step.original)
        } else if !step.original.exists() && step.target.exists() {
            fs::rename(&step.target, &step.original)
        } else {
            continue;
        };
        if let Err(error) = result {
            errors.push(format!("{}: {error}", step.original.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn temporary_path(source: &Path, index: usize) -> PathBuf {
    let directory = source.parent().unwrap_or_else(|| std::path::Path::new("."));
    let pid = std::process::id();
    let mut candidate = directory.join(format!(".fancy-renamer-tmp-{pid}-{index}"));
    let mut attempt = 0;
    while candidate.exists() {
        attempt += 1;
        candidate = directory.join(format!(".fancy-renamer-tmp-{pid}-{index}-{attempt}"));
    }
    candidate
}
