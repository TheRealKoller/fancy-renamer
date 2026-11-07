use fancy_renamer::engine::{
    FileGroup, FileItem, NumberingMode, NumberingRule, PlanStatus, Rule, RuleChain, RuleContext,
    RuleError, build_plan, execute,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn touch(dir: &Path, name: &str) -> FileItem {
    let path = dir.join(name);
    fs::write(&path, name).unwrap();
    FileItem::from_path(path).unwrap()
}

fn chain(rules: Vec<Box<dyn Rule>>) -> RuleChain {
    RuleChain::new(rules)
}

/// Testregel, die einen Text vorne anfügt.
#[derive(Debug)]
struct Prefix(&'static str);

impl Rule for Prefix {
    fn id(&self) -> &'static str {
        "test.prefix"
    }

    fn apply(&self, stem: &str, _ctx: &RuleContext<'_>) -> Result<String, RuleError> {
        Ok(format!("{}{stem}", self.0))
    }
}

#[test]
fn default_replace_gives_three_digit_numbers() {
    let dir = TempDir::new().unwrap();
    let items = [
        touch(dir.path(), "foto.jpg"),
        touch(dir.path(), "urlaub.jpg"),
    ];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(NumberingRule::with_defaults(
            NumberingMode::Replace,
        ))]),
    );
    assert_eq!(plan.rows[0].new_path, dir.path().join("001.jpg"));
    assert_eq!(plan.rows[1].new_path, dir.path().join("002.jpg"));
}

#[test]
fn default_append_uses_underscore() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "foto.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(NumberingRule::with_defaults(
            NumberingMode::Append,
        ))]),
    );
    assert_eq!(plan.rows[0].new_path, dir.path().join("foto_001.jpg"));
}

#[test]
fn start_step_and_digits_are_configurable() {
    let dir = TempDir::new().unwrap();
    let items = [
        touch(dir.path(), "a.jpg"),
        touch(dir.path(), "b.jpg"),
        touch(dir.path(), "c.jpg"),
    ];
    let rule = NumberingRule::new(NumberingMode::Replace, 5, 10, 4, "", "").unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(vec![Box::new(rule)]));
    assert_eq!(
        plan.rows
            .iter()
            .map(|row| row.new_stem.clone())
            .collect::<Vec<_>>(),
        vec!["0005", "0015", "0025"]
    );
}

#[test]
fn prefix_and_suffix_are_applied() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg")];
    let rule = NumberingRule::new(NumberingMode::Replace, 1, 1, 2, "img-", "-neu").unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(vec![Box::new(rule)]));
    assert_eq!(plan.rows[0].new_path, dir.path().join("img-01-neu.jpg"));
}

#[test]
fn numbers_beyond_digits_are_not_truncated() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg")];
    let rule = NumberingRule::new(NumberingMode::Replace, 1234, 1, 3, "", "").unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(vec![Box::new(rule)]));
    assert_eq!(plan.rows[0].new_stem, "1234");
}

#[test]
fn pair_members_share_one_number() {
    let dir = TempDir::new().unwrap();
    let items = [
        touch(dir.path(), "abc.jpg"),
        touch(dir.path(), "abc.raw"),
        touch(dir.path(), "def.jpg"),
    ];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(NumberingRule::with_defaults(
            NumberingMode::Replace,
        ))]),
    );
    assert_eq!(plan.rows[0].new_path, dir.path().join("001.jpg"));
    assert_eq!(plan.rows[1].new_path, dir.path().join("001.raw"));
    assert_eq!(plan.rows[2].new_path, dir.path().join("002.jpg"));

    let report = execute(&plan).unwrap();
    assert_eq!(report.renamed_count(), 3);
    assert!(dir.path().join("001.jpg").exists());
    assert!(dir.path().join("001.raw").exists());
    assert!(dir.path().join("002.jpg").exists());
}

#[test]
fn numbering_combines_with_replace_rule() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "IMG_2024.jpg")];
    let numbering = NumberingRule::new(NumberingMode::Append, 7, 2, 2, "_", "").unwrap();
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(Prefix("foto_")), Box::new(numbering)]),
    );
    assert_eq!(plan.rows[0].new_stem, "foto_IMG_2024_07");
}

#[test]
fn numbering_makes_unchanged_files_different() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "001.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(vec![Box::new(NumberingRule::with_defaults(
            NumberingMode::Replace,
        ))]),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Unchanged);
}

#[test]
fn zero_step_is_rejected() {
    let error = NumberingRule::new(NumberingMode::Replace, 1, 0, 3, "", "").unwrap_err();
    assert!(matches!(
        error,
        fancy_renamer::engine::RuleError::InvalidReplacement(_)
    ));
}
