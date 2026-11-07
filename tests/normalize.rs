use fancy_renamer::engine::{
    CaseMode, FileGroup, FileItem, NormalizeOptions, NormalizeRule, PlanStatus, Rule, RuleChain,
    build_plan, execute,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn touch(dir: &Path, name: &str) -> FileItem {
    let path = dir.join(name);
    fs::write(&path, name).unwrap();
    FileItem::from_path(path).unwrap()
}

fn plan_for(name: &str, options: NormalizeOptions) -> (TempDir, fancy_renamer::engine::RenamePlan) {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), name)];
    let chain = RuleChain::new(vec![Box::new(NormalizeRule::new(options))]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    (dir, plan)
}

fn stem(name: &str, options: NormalizeOptions) -> String {
    let (_dir, plan) = plan_for(name, options);
    plan.rows[0].new_stem.clone()
}

#[test]
fn case_modes() {
    let lower = NormalizeOptions {
        case: CaseMode::Lower,
        ..Default::default()
    };
    let upper = NormalizeOptions {
        case: CaseMode::Upper,
        ..Default::default()
    };
    let title = NormalizeOptions {
        case: CaseMode::Title,
        ..Default::default()
    };
    let sentence = NormalizeOptions {
        case: CaseMode::Sentence,
        ..Default::default()
    };
    assert_eq!(stem("FOTO.JPG", lower), "foto");
    assert_eq!(stem("foto", upper), "FOTO");
    assert_eq!(stem("mein tolles foto", title), "Mein Tolles Foto");
    assert_eq!(stem("mein tolles foto", sentence), "Mein tolles foto");
}

#[test]
fn spaces_become_single_underscore() {
    let options = NormalizeOptions {
        spaces_to_underscore: true,
        ..Default::default()
    };
    assert_eq!(stem("mein   tolles__foto", options), "mein_tolles_foto");
}

#[test]
fn separators_are_trimmed() {
    let options = NormalizeOptions {
        spaces_to_underscore: true,
        trim_separators: true,
        ..Default::default()
    };
    assert_eq!(stem("  _foto_  ", options), "foto");
}

#[test]
fn transliteration_of_umlauts_and_sharp_s() {
    let options = NormalizeOptions {
        transliterate: true,
        ..Default::default()
    };
    assert_eq!(stem("Grüße für Öl", options), "Gruesse fuer Oel");
}

#[test]
fn special_characters_are_stripped_but_unicode_letters_kept() {
    let options = NormalizeOptions {
        strip_special: true,
        ..Default::default()
    };
    assert_eq!(stem("foto (1) #2! ü.jpg", options), "foto12ü");
}

#[test]
fn cleanup_is_idempotent() {
    let options = NormalizeOptions {
        case: CaseMode::Title,
        spaces_to_underscore: true,
        trim_separators: true,
        transliterate: true,
        strip_special: true,
    };
    let once = stem("  ____Grüße   aus  dem   URLAUB (2024)!!___  ", options);
    let twice = stem(&once, options);
    assert_eq!(once, twice, "zweite Anwendung darf nichts mehr ändern");
}

#[test]
fn name_of_only_special_characters_is_invalid() {
    let options = NormalizeOptions {
        strip_special: true,
        ..Default::default()
    };
    let (_dir, plan) = plan_for("!!!.jpg", options);
    assert_eq!(plan.rows[0].new_stem, "");
    assert_eq!(plan.rows[0].status, PlanStatus::InvalidName);
    assert!(!plan.is_executable());
}

#[test]
fn extension_is_never_touched() {
    let options = NormalizeOptions {
        case: CaseMode::Upper,
        ..Default::default()
    };
    let (_dir, plan) = plan_for("bild.jpg", options);
    assert_eq!(plan.rows[0].new_path.file_name().unwrap(), "BILD.jpg");
}

#[test]
fn normalize_combined_with_other_rules_executes() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "Mein Foto.JPG")];
    let chain = RuleChain::new(vec![Box::new(NormalizeRule::new(NormalizeOptions {
        case: CaseMode::Lower,
        spaces_to_underscore: true,
        ..Default::default()
    }))]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert_eq!(plan.rows[0].new_path, dir.path().join("mein_foto.JPG"));
    execute(&plan).unwrap();
    assert!(dir.path().join("mein_foto.JPG").exists());
}

#[test]
fn noop_options_leave_name_unchanged() {
    let (_dir, plan) = plan_for("foto.jpg", NormalizeOptions::default());
    assert_eq!(plan.rows[0].status, PlanStatus::Unchanged);
}

#[test]
fn rule_trait_is_implemented() {
    let rule = NormalizeRule::new(NormalizeOptions::default());
    assert_eq!(rule.id(), "normalize");
    assert!(rule.options().is_noop());
}
