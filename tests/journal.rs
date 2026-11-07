use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use fancy_renamer::engine::{
    FileGroup, FileItem, Journal, JournalEntry, ReplaceRule, Rule, RuleChain, RuleContext,
    RuleError, RuleOutcome, build_plan, execute, undo,
};
use tempfile::TempDir;

fn touch(dir: &Path, name: &str) -> FileItem {
    let path = dir.join(name);
    fs::write(&path, name).unwrap();
    FileItem::from_path(path).unwrap()
}

/// Testregel: bildet alte Stämme auf neue ab.
#[derive(Debug)]
struct MapTargets(Vec<(String, String)>);

impl Rule for MapTargets {
    fn id(&self) -> &'static str {
        "test.map"
    }

    fn apply(&self, stem: &str, _ctx: &RuleContext<'_>) -> Result<RuleOutcome, RuleError> {
        self.0
            .iter()
            .find(|(old, _)| old == stem)
            .map(|(_, new)| RuleOutcome::Stem(new.clone()))
            .ok_or_else(|| RuleError::InvalidPattern(format!("kein Ziel für {stem}")))
    }
}

/// Benennt `alt` in `neu` um und liefert den Journaleintrag dazu.
fn rename_and_record(dir: &Path, pairs: &[(&str, &str)]) -> JournalEntry {
    let items = pairs
        .iter()
        .map(|(old, _)| touch(dir, old))
        .collect::<Vec<_>>();
    let map = MapTargets(
        pairs
            .iter()
            .map(|(old, new)| {
                let old = old.rsplit_once('.').map_or(*old, |(stem, _)| stem);
                let new = new.rsplit_once('.').map_or(*new, |(stem, _)| stem);
                (old.to_string(), new.to_string())
            })
            .collect(),
    );
    let chain = RuleChain::new(vec![Box::new(map)]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    let report = execute(&plan).unwrap();
    JournalEntry::from_report(&report).unwrap()
}

#[test]
fn undo_restores_the_previous_names() {
    let dir = TempDir::new().unwrap();
    let entry = rename_and_record(
        dir.path(),
        &[("a.jpg", "neu_a.jpg"), ("b.jpg", "neu_b.jpg")],
    );
    assert!(dir.path().join("neu_a.jpg").exists());

    let report = undo(&entry).unwrap();
    assert_eq!(report.restored_count(), 2);
    assert!(report.skipped.is_empty());
    assert!(dir.path().join("a.jpg").exists());
    assert!(dir.path().join("b.jpg").exists());
    assert!(!dir.path().join("neu_a.jpg").exists());
    assert!(!dir.path().join(".fancy-renamer-undo-0").exists());
}

#[test]
fn undo_skips_files_changed_after_the_rename() {
    let dir = TempDir::new().unwrap();
    let entry = rename_and_record(
        dir.path(),
        &[("a.jpg", "neu_a.jpg"), ("b.jpg", "neu_b.jpg")],
    );

    let changed = dir.path().join("neu_b.jpg");
    fs::write(&changed, "viel längerer Inhalt").unwrap();
    let later = SystemTime::now() + Duration::from_secs(5);
    fs::File::options()
        .write(true)
        .open(&changed)
        .unwrap()
        .set_modified(later)
        .unwrap();

    let report = undo(&entry).unwrap();
    assert_eq!(report.restored_count(), 1);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].path, changed);
    assert!(dir.path().join("a.jpg").exists());
    assert!(changed.exists());
    assert!(!dir.path().join("b.jpg").exists());
}

#[test]
fn undo_skips_when_the_original_name_is_taken() {
    let dir = TempDir::new().unwrap();
    let entry = rename_and_record(dir.path(), &[("a.jpg", "neu_a.jpg")]);
    fs::write(dir.path().join("a.jpg"), "belegt").unwrap();

    let report = undo(&entry).unwrap();
    assert_eq!(report.restored_count(), 0);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].reason, "Originalpfad ist belegt");
    assert!(dir.path().join("neu_a.jpg").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("a.jpg")).unwrap(),
        "belegt"
    );
}

#[test]
fn undo_skips_missing_targets() {
    let dir = TempDir::new().unwrap();
    let entry = rename_and_record(dir.path(), &[("a.jpg", "neu_a.jpg")]);
    fs::remove_file(dir.path().join("neu_a.jpg")).unwrap();

    let report = undo(&entry).unwrap();
    assert_eq!(report.restored_count(), 0);
    assert_eq!(report.skipped[0].reason, "Zieldatei fehlt");
}

#[test]
fn journal_roundtrip_keeps_entries() {
    let dir = TempDir::new().unwrap();
    let journal_path = dir.path().join("state").join("journal.json");
    let entry = rename_and_record(dir.path(), &[("a.jpg", "neu_a.jpg")]);

    let mut journal = Journal::default();
    journal.push(entry.clone());
    journal.save(&journal_path).unwrap();

    let loaded = Journal::load(&journal_path);
    assert_eq!(loaded.entries, vec![entry]);
    assert_eq!(
        loaded.last().unwrap().renames[0].from,
        dir.path().join("a.jpg")
    );
}

#[test]
fn broken_or_missing_journal_reads_as_empty() {
    let dir = TempDir::new().unwrap();
    let journal_path = dir.path().join("journal.json");
    assert!(Journal::load(&journal_path).is_empty());

    fs::write(&journal_path, "{kaputt").unwrap();
    assert!(Journal::load(&journal_path).is_empty());
}

#[test]
fn journal_keeps_only_the_last_twenty_runs() {
    let dir = TempDir::new().unwrap();
    let mut journal = Journal::default();
    for index in 0..25 {
        let entry = rename_and_record(
            dir.path(),
            &[(
                format!("f{index}.jpg").as_str(),
                format!("n{index}.jpg").as_str(),
            )],
        );
        journal.push(entry);
    }
    assert_eq!(journal.entries.len(), 20);
    assert_eq!(
        journal.last().unwrap().renames[0].from,
        dir.path().join("f24.jpg")
    );
}

#[test]
fn undo_handles_name_cycles() {
    let dir = TempDir::new().unwrap();
    let entry = rename_and_record(dir.path(), &[("a.jpg", "b.jpg"), ("b.jpg", "a.jpg")]);
    let report = undo(&entry).unwrap();
    assert_eq!(report.restored_count(), 2);
    assert!(dir.path().join("a.jpg").exists());
    assert!(dir.path().join("b.jpg").exists());
}

#[test]
fn empty_report_produces_no_journal_entry() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "a.jpg")];
    let plan = build_plan(&FileGroup::from_items(&items), &RuleChain::new(Vec::new()));
    let report = execute(&plan).unwrap();
    assert!(JournalEntry::from_report(&report).is_none());
}

#[test]
fn replace_rule_plan_can_be_undone() {
    let dir = TempDir::new().unwrap();
    let items = [touch(dir.path(), "IMG_1.jpg")];
    let rule = ReplaceRule::new(
        fancy_renamer::engine::MatchMode::Literal,
        "IMG_".to_string(),
        "foto_".to_string(),
        false,
        false,
    )
    .unwrap();
    let chain = RuleChain::new(vec![Box::new(rule)]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    let report = execute(&plan).unwrap();
    let entry = JournalEntry::from_report(&report).unwrap();
    assert!(dir.path().join("foto_1.jpg").exists());

    undo(&entry).unwrap();
    assert!(dir.path().join("IMG_1.jpg").exists());
    assert!(!dir.path().join("foto_1.jpg").exists());
}
