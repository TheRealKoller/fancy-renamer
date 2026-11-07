use std::fs;
use std::path::Path;

use fancy_renamer::engine::{
    ExecuteError, FileGroup, FileItem, PlanStatus, Rule, RuleChain, RuleContext, RuleError,
    RuleOutcome, build_plan, execute,
};
use tempfile::TempDir;

/// Testregel: ersetzt wörtlich alle Vorkommen.
#[derive(Debug)]
struct Replace(&'static str, &'static str);

impl Rule for Replace {
    fn id(&self) -> &'static str {
        "test.replace"
    }

    fn apply(&self, stem: &str, _ctx: &RuleContext<'_>) -> Result<RuleOutcome, RuleError> {
        Ok(RuleOutcome::Stem(stem.replace(self.0, self.1)))
    }
}

/// Testregel: benennt nach Position um; erlaubt damit Zyklen zu bauen.
#[derive(Debug)]
struct FixedTargets(Vec<String>);

impl Rule for FixedTargets {
    fn id(&self) -> &'static str {
        "test.fixed"
    }

    fn apply(&self, _stem: &str, ctx: &RuleContext<'_>) -> Result<RuleOutcome, RuleError> {
        Ok(RuleOutcome::Stem(self.0[ctx.index].clone()))
    }
}

#[derive(Debug)]
struct FailingRule;

impl Rule for FailingRule {
    fn id(&self) -> &'static str {
        "test.failing"
    }

    fn apply(&self, _stem: &str, _ctx: &RuleContext<'_>) -> Result<RuleOutcome, RuleError> {
        Err(RuleError::InvalidPattern("kaputt".into()))
    }
}

fn touch(dir: &Path, name: &str) -> FileItem {
    let path = dir.join(name);
    fs::write(&path, name).unwrap();
    FileItem::from_path(path).unwrap()
}

fn chain(rules: Vec<Box<dyn Rule>>) -> RuleChain {
    RuleChain::new(rules)
}

#[test]
fn empty_list_yields_empty_plan() {
    let plan = build_plan(&[], &chain(vec![]));
    assert!(plan.rows.is_empty());
    assert!(plan.is_executable());
    assert_eq!(plan.rename_rows(), 0);
}

#[test]
fn empty_chain_marks_everything_unchanged() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "foto.jpg")];
    let plan = build_plan(&FileGroup::from_items(&items), &chain(vec![]));
    assert_eq!(plan.rows[0].status, PlanStatus::Unchanged);
    assert_eq!(plan.rows[0].new_path, items[0].path);
}

#[test]
fn replace_rule_keeps_extension_and_counts_rename() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "urlaub-2024.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(Replace("-", "_"))]),
    );
    let row = &plan.rows[0];
    assert_eq!(row.status, PlanStatus::Ok);
    assert_eq!(
        row.new_path,
        dir.path().join("urlaub_2024.jpg"),
        "Erweiterung bleibt erhalten"
    );
    assert_eq!(plan.rename_rows(), 1);
    assert!(plan.is_executable());
}

#[test]
fn rules_apply_in_order() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a-b.jpg")];
    let chain = chain(vec![
        Box::new(Replace("-", "_")),
        Box::new(Replace("_", "+")),
    ]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].new_stem, "a+b");
}

#[test]
fn invalid_rule_blocks_the_plan() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FailingRule)]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::RuleError);
    assert!(!plan.is_executable());
    assert_eq!(plan.blocking_rows(), 1);
}

#[test]
fn existing_target_is_a_collision() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg")];
    touch(dir.path(), "b.jpg");
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec!["b".to_owned()]))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Collision);
    assert!(!plan.is_executable());
}

#[test]
fn duplicate_targets_are_detected() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg"), touch(dir.path(), "b.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(Replace("a", "x"))]),
    );
    // a.jpg -> x.jpg, b.jpg bleibt unverändert: kein Duplikat.
    assert_eq!(plan.rows[0].status, PlanStatus::Ok);
    assert_eq!(plan.rows[1].status, PlanStatus::Unchanged);

    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec![
            "gleich".to_owned(),
            "gleich".to_owned(),
        ]))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Duplicate);
    assert_eq!(plan.rows[1].status, PlanStatus::Duplicate);
    assert!(!plan.is_executable());
}

#[test]
fn name_cycle_is_executable() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg"), touch(dir.path(), "b.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec![
            "b".to_owned(),
            "a".to_owned(),
        ]))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Ok);
    assert_eq!(plan.rows[1].status, PlanStatus::Ok);

    let report = execute(&plan).unwrap();
    assert_eq!(report.renamed_count(), 2);
    assert_eq!(
        fs::read_to_string(dir.path().join("a.jpg")).unwrap(),
        "b.jpg"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("b.jpg")).unwrap(),
        "a.jpg"
    );
    assert!(!dir.path().join(".fancy-renamer-tmp-0").exists());
}

#[test]
fn execute_refuses_blocked_plan() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg")];
    touch(dir.path(), "b.jpg");
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec!["b".to_owned()]))]),
    );
    let error = execute(&plan).unwrap_err();
    assert!(matches!(error, ExecuteError::Blocked(1)));
    assert_eq!(
        fs::read_to_string(dir.path().join("a.jpg")).unwrap(),
        "a.jpg"
    );
}

#[test]
fn execute_renames_and_reports() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg"), touch(dir.path(), "b.raw")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(Replace("a", "neu"))]),
    );
    let report = execute(&plan).unwrap();
    assert_eq!(report.renamed_count(), 1);
    assert_eq!(report.skipped, 1);
    assert!(dir.path().join("neu.jpg").exists());
    assert!(!dir.path().join("a.jpg").exists());
}

#[test]
fn stale_plan_is_rejected_without_changes() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec!["neu".to_owned()]))]),
    );

    fs::write(dir.path().join("a.jpg"), "inzwischen geändert").unwrap();
    let error = execute(&plan).unwrap_err();
    assert!(matches!(error, ExecuteError::StalePlan(_)));
    assert!(dir.path().join("a.jpg").exists());
    assert!(!dir.path().join("neu.jpg").exists());
}

#[test]
fn target_created_after_preview_triggers_rollback() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg"), touch(dir.path(), "b.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec![
            "neu-a".to_owned(),
            "neu-b".to_owned(),
        ]))]),
    );
    // Ziel des zweiten Schritts entsteht erst nach der Vorschau.
    fs::create_dir(dir.path().join("neu-b.jpg")).unwrap();

    let error = execute(&plan).unwrap_err();
    assert!(matches!(error, ExecuteError::TargetExists(_)));
    assert_eq!(
        fs::read_to_string(dir.path().join("a.jpg")).unwrap(),
        "a.jpg"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("b.jpg")).unwrap(),
        "b.jpg"
    );
    assert!(!dir.path().join("neu-a.jpg").exists());
    std::fs::remove_dir(dir.path().join("neu-b.jpg")).unwrap();
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn case_only_change_is_marked_and_allowed() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "foto.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec!["FOTO".to_owned()]))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::CaseOnly);
    assert!(plan.is_executable());

    execute(&plan).unwrap();
    assert!(dir.path().join("FOTO.jpg").exists());
}

#[test]
fn invalid_names_are_blocking() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec!["".to_owned()]))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::InvalidName);

    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec!["mit/slash".to_owned()]))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::InvalidName);

    let long = "x".repeat(256);
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec![long.clone()]))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::InvalidName);
}

#[test]
fn name_of_exactly_255_bytes_is_allowed() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg")];
    // 255 Byte inklusive ".jpg" = 251 Zeichen Stamm.
    let stem = "x".repeat(251);
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(FixedTargets(vec![stem.clone()]))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Ok);
}

#[test]
fn unicode_and_spaces_survive_rename() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "Grüße 2024 äöü 😀.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(Replace("2024", "2025"))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Ok);
    execute(&plan).unwrap();
    assert!(dir.path().join("Grüße 2025 äöü 😀.jpg").exists());
}

#[test]
fn pair_shares_new_stem_and_keeps_extensions() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "abc.jpg"), touch(dir.path(), "abc.raw")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(Replace("abc", "neuername"))]),
    );
    assert_eq!(plan.rows.len(), 2);
    assert_eq!(plan.rows[0].new_stem, "neuername");
    assert_eq!(plan.rows[1].new_stem, "neuername");
    assert_eq!(plan.rows[0].new_path, dir.path().join("neuername.jpg"));
    assert_eq!(plan.rows[1].new_path, dir.path().join("neuername.raw"));
    assert!(plan.is_executable());

    let report = execute(&plan).unwrap();
    assert_eq!(report.renamed_count(), 2);
    assert!(dir.path().join("neuername.jpg").exists());
    assert!(dir.path().join("neuername.raw").exists());
}

#[test]
fn group_of_three_shares_stem() {
    let dir = TempDir::new().unwrap();
    let items = [
        touch(dir.path(), "abc.jpg"),
        touch(dir.path(), "abc.raw"),
        touch(dir.path(), "abc.xmp"),
    ];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(Replace("abc", "x"))]),
    );
    assert_eq!(plan.rename_rows(), 3);
    assert_eq!(
        plan.rows
            .iter()
            .map(|row| row
                .new_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned())
            .collect::<Vec<_>>(),
        vec!["x.jpg", "x.raw", "x.xmp"]
    );
}

#[test]
fn unpaired_file_is_renamed_with_its_own_group() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "abc.jpg"), touch(dir.path(), "def.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(Replace("abc", "x"))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Ok);
    assert_eq!(plan.rows[1].status, PlanStatus::Unchanged);
}

#[test]
fn collision_is_checked_on_full_target_path() {
    let dir = TempDir::new().unwrap();
    // Zielstamm "def" existiert als def.jpg, aber nicht als def.raw.
    let items = [
        touch(dir.path(), "abc.jpg"),
        touch(dir.path(), "abc.raw"),
        touch(dir.path(), "def.jpg"),
    ];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(Replace("abc", "def"))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Collision);
    assert_eq!(plan.rows[1].status, PlanStatus::Ok);
    assert!(!plan.is_executable());
}

#[test]
fn identical_stem_with_same_extension_is_not_grouped_twice() {
    // Zwei Gruppen mit demselben Namen können nicht entstehen; Dateipfade sind eindeutig.
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "abc.jpg"), touch(dir.path(), "abc.jpg")];
    let groups = FileGroup::from_items(&items);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members.len(), 2);
}
