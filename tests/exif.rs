use fancy_renamer::engine::{
    ExifDateRule, ExifMode, FileGroup, FileItem, PlanStatus, RuleChain, build_plan, execute,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const FIXTURE_WITH_EXIF: &str = "tests/fixtures/exif.jpg";
const FIXTURE_WITHOUT_EXIF: &str = "tests/fixtures/ohne-exif.jpg";
const FIXTURE_TIFF: &str = "tests/fixtures/ohne-exif.tiff";

/// Fixture in ein temporäres Verzeichnis kopieren, damit die Tests die Dateien
/// umbenennen dürfen.
fn copy_fixture(dir: &Path, fixture: &str, name: &str) -> FileItem {
    let target = dir.join(name);
    fs::copy(fixture, &target).unwrap();
    FileItem::from_path(target).unwrap()
}

fn chain(rule: ExifDateRule) -> RuleChain {
    RuleChain::new(vec![Box::new(rule)])
}

#[test]
fn default_format_replaces_stem() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_WITH_EXIF, "IMG_0001.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(ExifDateRule::with_defaults(ExifMode::Replace)),
    );
    assert_eq!(
        plan.rows[0].new_path,
        dir.path().join("2023-07-14_09-15-22.jpg")
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Ok);
}

#[test]
fn prefix_mode_keeps_old_stem() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_WITH_EXIF, "IMG_0001.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(ExifDateRule::with_defaults(ExifMode::Prefix)),
    );
    assert_eq!(
        plan.rows[0].new_path,
        dir.path().join("2023-07-14_09-15-22_IMG_0001.jpg")
    );
}

#[test]
fn custom_format_is_used() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_WITH_EXIF, "foto.jpg")];
    let rule = ExifDateRule::new("%Y%m%d", ExifMode::Replace).unwrap();
    let plan = build_plan(&FileGroup::from_items(&items), &chain(rule));
    assert_eq!(plan.rows[0].new_stem, "20230714");
}

#[test]
fn file_without_exif_is_skipped_and_kept() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_WITHOUT_EXIF, "ohne.jpg")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(ExifDateRule::with_defaults(ExifMode::Replace)),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Skipped);
    assert_eq!(plan.rows[0].note.as_deref(), Some("keine EXIF-Daten"));
    assert_eq!(plan.rows[0].new_path, items[0].path);
    assert!(
        plan.is_executable(),
        "übersprungene Dateien blockieren nicht"
    );
    assert_eq!(plan.rename_rows(), 0);

    let report = execute(&plan).unwrap();
    assert_eq!(report.renamed_count(), 0);
    assert!(dir.path().join("ohne.jpg").exists());
}

#[test]
fn unreadable_file_is_skipped_not_an_error() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("kaputt.jpg");
    fs::write(&path, b"kein bild").unwrap();
    let items = [FileItem::from_path(&path).unwrap()];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(ExifDateRule::with_defaults(ExifMode::Replace)),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Skipped);
    assert!(plan.is_executable());
}

#[test]
fn tiff_without_date_is_skipped() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_TIFF, "scan.tiff")];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(ExifDateRule::with_defaults(ExifMode::Replace)),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Skipped);
}

#[test]
fn identical_timestamps_collide_and_block() {
    let dir = TempDir::new().unwrap();
    let items = [
        copy_fixture(dir.path(), FIXTURE_WITH_EXIF, "a.jpg"),
        copy_fixture(dir.path(), FIXTURE_WITH_EXIF, "b.jpg"),
    ];
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(ExifDateRule::with_defaults(ExifMode::Replace)),
    );
    assert_eq!(plan.rows[0].status, PlanStatus::Duplicate);
    assert_eq!(plan.rows[1].status, PlanStatus::Duplicate);
    assert!(!plan.is_executable());
}

#[test]
fn numbering_breaks_timestamp_collisions() {
    use fancy_renamer::engine::{NumberingMode, NumberingRule};

    let dir = TempDir::new().unwrap();
    let items = [
        copy_fixture(dir.path(), FIXTURE_WITH_EXIF, "a.jpg"),
        copy_fixture(dir.path(), FIXTURE_WITH_EXIF, "b.jpg"),
    ];
    let chain = RuleChain::new(vec![
        Box::new(ExifDateRule::with_defaults(ExifMode::Replace)),
        Box::new(NumberingRule::new(NumberingMode::Append, 1, 1, 2, "_", "").unwrap()),
    ]);
    let plan = build_plan(&FileGroup::from_items(&items), &chain);
    assert!(plan.is_executable());
    let names: Vec<String> = plan
        .rows
        .iter()
        .map(|row| {
            row.new_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        names,
        vec!["2023-07-14_09-15-22_01.jpg", "2023-07-14_09-15-22_02.jpg"]
    );
}

#[test]
fn rename_does_not_change_content() {
    let dir = TempDir::new().unwrap();
    let items = [copy_fixture(dir.path(), FIXTURE_WITH_EXIF, "foto.jpg")];
    let original = fs::read(items[0].path.clone()).unwrap();
    let plan = build_plan(
        &FileGroup::from_items(&items),
        &chain(ExifDateRule::with_defaults(ExifMode::Replace)),
    );
    execute(&plan).unwrap();
    let renamed = fs::read(dir.path().join("2023-07-14_09-15-22.jpg")).unwrap();
    assert_eq!(original, renamed);
}

#[test]
fn invalid_date_format_is_rejected() {
    let error = ExifDateRule::new("%Q", ExifMode::Replace).unwrap_err();
    assert!(matches!(
        error,
        fancy_renamer::engine::RuleError::InvalidPattern(_)
    ));
}

#[test]
fn fixture_paths_exist() {
    for fixture in [FIXTURE_WITH_EXIF, FIXTURE_WITHOUT_EXIF, FIXTURE_TIFF] {
        assert!(PathBuf::from(fixture).exists(), "{fixture} fehlt");
    }
}
