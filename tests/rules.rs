use fancy_renamer::engine::{
    FileGroup, FileItem, MatchMode, PlanStatus, ReplaceRule, RuleChain, build_plan, execute,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn touch(dir: &Path, name: &str) -> FileItem {
    let path = dir.join(name);
    fs::write(&path, name).unwrap();
    FileItem::from_path(path).unwrap()
}

fn rule(
    mode: MatchMode,
    pattern: &str,
    replacement: &str,
    case_insensitive: bool,
    first_only: bool,
) -> Box<ReplaceRule> {
    Box::new(
        ReplaceRule::new(mode, pattern, replacement, case_insensitive, first_only)
            .expect("Muster muss gültig sein"),
    )
}

#[test]
fn literal_replaces_all_occurrences_in_stem_only() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a-a-a.jpg")];
    let chain = RuleChain::new(vec![rule(MatchMode::Literal, "a", "b", false, false)]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].new_path, dir.path().join("b-b-b.jpg"));
}

#[test]
fn literal_does_not_touch_extension() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "bild.jpg")];
    let chain = RuleChain::new(vec![rule(MatchMode::Literal, "jpg", "png", false, false)]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].status, PlanStatus::Unchanged);
    assert_eq!(plan.rows[0].new_path, dir.path().join("bild.jpg"));
}

#[test]
fn literal_replacement_is_not_a_regex_template() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "preis.jpg")];
    let chain = RuleChain::new(vec![rule(
        MatchMode::Literal,
        "preis",
        "$1 \\ $",
        false,
        false,
    )]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].new_stem, "$1 \\ $");
}

#[test]
fn literal_case_insensitive() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "Foto.jpg")];
    let chain = RuleChain::new(vec![rule(MatchMode::Literal, "foto", "bild", true, false)]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].new_stem, "bild");
}

#[test]
fn literal_first_only() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "aaa.jpg")];
    let chain = RuleChain::new(vec![rule(MatchMode::Literal, "a", "b", false, true)]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].new_stem, "baa");
}

#[test]
fn regex_captures() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "2024-05-01_urlaub.jpg")];
    let chain = RuleChain::new(vec![rule(
        MatchMode::Regex,
        r"(\d{4})-(\d{2})-(\d{2})",
        "${3}.${2}.${1}",
        false,
        false,
    )]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].new_stem, "01.05.2024_urlaub");
}

#[test]
fn regex_case_insensitive_and_first_only() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "Foo foo.jpg")];
    let chain = RuleChain::new(vec![rule(MatchMode::Regex, "foo", "bar", true, true)]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].new_stem, "bar foo");
}

#[test]
fn invalid_regex_is_reported_when_building_the_rule() {
    let error = ReplaceRule::new(MatchMode::Regex, "([a-", "x", false, false).unwrap_err();
    assert!(matches!(
        error,
        fancy_renamer::engine::RuleError::InvalidPattern(_)
    ));
}

#[test]
fn empty_pattern_is_noop() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "foto.jpg")];
    let chain = RuleChain::new(vec![rule(MatchMode::Literal, "", "x", false, false)]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].status, PlanStatus::Unchanged);
}

#[test]
fn unicode_replacement() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "Grüße.jpg")];
    let chain = RuleChain::new(vec![rule(MatchMode::Literal, "ü", "ue", false, false)]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].new_stem, "Grueße");

    let plan = build_plan(
        &FileGroup::from_items(&items),
        &RuleChain::new(vec![rule(MatchMode::Regex, "ß", "ss", false, false)]),
    );
    let report = execute(&plan).unwrap();
    assert_eq!(report.renamed_count(), 1);
    assert!(dir.path().join("Grüsse.jpg").exists());
}
