use fancy_renamer::engine::{
    FileGroup, FileItem, Id3Mode, Id3TagRule, PlanStatus, RuleChain, build_plan, execute,
};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const FIXTURE_V23: &str = "tests/fixtures/id3v23.mp3";
const FIXTURE_V1: &str = "tests/fixtures/id3v1.mp3";
const FIXTURE_WITHOUT_TRACK: &str = "tests/fixtures/id3v23-ohne-track.mp3";
const FIXTURE_BROKEN: &str = "tests/fixtures/id3-broken.mp3";

/// Fixture in ein temporäres Verzeichnis kopieren, damit die Tests die Dateien
/// umbenennen dürfen.
fn copy_fixture(dir: &Path, fixture: &str, name: &str) -> FileItem {
    let target = dir.join(name);
    fs::copy(fixture, &target).unwrap();
    FileItem::from_path(target).unwrap()
}

fn chain(rule: Id3TagRule) -> RuleChain {
    RuleChain::new(vec![Box::new(rule)])
}

#[test]
fn id3v23_tags_build_artist_and_title() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_V23, "track01.mp3")];
    let rule = Id3TagRule::new("{artist} - {title}", Id3Mode::Replace).unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(rule));
    assert_eq!(
        plan.rows[0].new_path,
        dir.path().join("Böhse Onkelz - Weiß.mp3")
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Ok);
}

#[test]
fn all_placeholders_fill_from_tags() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_V23, "track01.mp3")];
    let rule =
        Id3TagRule::new("{artist}_{album}_{year}_{track}_{title}", Id3Mode::Replace).unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(rule));
    assert_eq!(
        plan.rows[0].new_path,
        dir.path()
            .join("Böhse Onkelz_Viva los Tioz_1998_07_Weiß.mp3")
    );
}

#[test]
fn missing_tags_are_skipped_without_blocking() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_BROKEN, "kaputt.mp3")];
    let rule = Id3TagRule::with_defaults(Id3Mode::Replace);
    let plan = build_plan(&FileGroup::from_items(&items), &chain(rule));
    assert_eq!(plan.rows[0].status, PlanStatus::Skipped);
    assert!(plan.is_executable());
    assert_eq!(plan.rename_rows(), 0);
}

#[test]
fn id3v1_only_files_are_read_with_latin1_umlauts() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_V1, "v1.mp3")];
    let rule = Id3TagRule::new("{artist} - {title}", Id3Mode::Replace).unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(rule));
    assert_eq!(
        plan.rows[0].new_path,
        dir.path().join("Böhse Onkelz - Weiß.mp3")
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Ok);
}

#[test]
fn missing_placeholder_leaves_no_double_separators() {
    let dir = TempDir::new().unwrap();
    // Fixture mit ID3v2.3, aber ohne TRCK: Track bleibt leer.
    let items = [copy_fixture(
        dir.path(),
        FIXTURE_WITHOUT_TRACK,
        "track01.mp3",
    )];
    let rule = Id3TagRule::new("{artist} - {album} - {track}", Id3Mode::Replace).unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(rule));
    assert_eq!(
        plan.rows[0].new_path,
        dir.path().join("Böhse Onkelz - Viva los Tioz.mp3")
    );
}

#[test]
fn track_number_is_padded_to_two_digits() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_V23, "track01.mp3")];
    let rule = Id3TagRule::new("track{track}", Id3Mode::Replace).unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(rule));
    assert_eq!(plan.rows[0].new_path, dir.path().join("track07.mp3"));
}

#[test]
fn prefix_mode_keeps_old_stem() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_V23, "track01.mp3")];
    let rule = Id3TagRule::new("{artist}", Id3Mode::Prefix).unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(rule));
    assert_eq!(
        plan.rows[0].new_path,
        dir.path().join("Böhse Onkelz_track01.mp3")
    );
}

#[test]
fn unknown_placeholder_is_rejected() {
    let error = Id3TagRule::new("{artist} {genre}", Id3Mode::Replace).unwrap_err();
    assert!(error.to_string().contains("genre"), "{error}");
}

#[test]
fn rule_renames_and_extension_stays() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_V1, "Musik.mp3")];
    let rule = Id3TagRule::new("{title}", Id3Mode::Replace).unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(rule));
    execute(&plan).unwrap();
    assert!(dir.path().join("Weiß.mp3").exists());
}
