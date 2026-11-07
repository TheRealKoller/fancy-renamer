//! Engine-Tests für Presets: Serialisierung, Datei, Verwaltung, Kette.

use std::path::Path;

use fancy_renamer::engine::{
    CaseMode, ExifMode, Id3Mode, MatchMode, MergeStats, NumberingMode, Preset, Presets,
    RuleContext, RuleOutcome, RulePreset, validate_name,
};
use tempfile::TempDir;

fn context() -> RuleContext<'static> {
    RuleContext {
        path: Path::new("foto.jpg"),
        original_stem: "foto",
        index: 0,
        group_index: 0,
        group_size: 1,
        member_index: 0,
    }
}

fn all_rules() -> Vec<RulePreset> {
    vec![
        RulePreset::Replace {
            mode: MatchMode::Regex,
            pattern: r"(\d{4})-(\d{2})".into(),
            replacement: "${2}_${1}".into(),
            case_insensitive: true,
            first_only: true,
        },
        RulePreset::Numbering {
            mode: NumberingMode::Append,
            start: 7,
            step: 3,
            digits: 4,
            prefix: "-".into(),
            suffix: "x".into(),
        },
        RulePreset::Normalize {
            case: CaseMode::Title,
            spaces_to_underscore: true,
            trim_separators: true,
            transliterate: true,
            strip_special: false,
        },
        RulePreset::ExifDate {
            format: "%Y-%m".into(),
            mode: ExifMode::Prefix,
        },
        RulePreset::Id3Tags {
            format: "{track} - {artist} - {title}".into(),
            mode: Id3Mode::Replace,
        },
    ]
}

fn sample_preset(name: &str) -> Preset {
    Preset {
        name: name.into(),
        rules: all_rules(),
    }
}

#[test]
fn every_rule_kind_survives_a_json_roundtrip() {
    let mut presets = Presets::default();
    presets.upsert(sample_preset("Alle")).unwrap();
    let json = presets.to_json().unwrap();
    let restored = Presets::from_json(&json).unwrap();
    assert_eq!(restored, presets);
    assert_eq!(restored.entries()[0].rules, all_rules());
}

#[test]
fn preset_builds_a_working_chain_in_order() {
    let mut preset = sample_preset("Kette");
    preset.rules.truncate(3);
    let chain = preset.build_chain().unwrap();
    assert_eq!(chain.ids(), vec!["replace", "numbering", "normalize"]);
    let outcome = chain.apply("bild-2024-05", &context()).unwrap();
    // Replace (Regex, erstes Vorkommen, benannte Captures) → "bild-05_2024",
    // Numbering hängt "-0007x" an, Normalize macht den Wortanfang groß.
    assert_eq!(outcome, RuleOutcome::Stem("Bild-05_2024-0007x".into()));
}

#[test]
fn preset_with_invalid_regex_fails_to_build() {
    let preset = Preset {
        name: "Kaputt".into(),
        rules: vec![RulePreset::Replace {
            mode: MatchMode::Regex,
            pattern: "(".into(),
            replacement: "".into(),
            case_insensitive: false,
            first_only: false,
        }],
    };
    assert!(preset.build_chain().is_err());
}

#[test]
fn preset_with_zero_step_fails_to_build() {
    let preset = Preset {
        name: "Kaputt".into(),
        rules: vec![RulePreset::Numbering {
            mode: NumberingMode::Replace,
            start: 1,
            step: 0,
            digits: 3,
            prefix: String::new(),
            suffix: String::new(),
        }],
    };
    assert!(preset.build_chain().is_err());
}

#[test]
fn missing_or_broken_file_reads_as_empty() {
    let dir = TempDir::new().unwrap();
    assert!(Presets::load(&dir.path().join("fehlt.json")).is_empty());
    let broken = dir.path().join("kaputt.json");
    std::fs::write(&broken, b"{ nicht json").unwrap();
    assert!(Presets::load(&broken).is_empty());
}

#[test]
fn invalid_json_is_reported_on_import() {
    assert!(Presets::from_json("{").is_err());
    assert!(Presets::from_json("").is_err());
}

#[test]
fn invalid_and_duplicate_entries_are_dropped() {
    let json = r#"
    {
      "version": 1,
      "presets": [
        { "name": "  ", "rules": [] },
        { "name": "Doppelt", "rules": [] },
        { "name": "Doppelt", "rules": [] }
      ]
    }
    "#;
    let presets = Presets::from_json(json).unwrap();
    assert_eq!(presets.names(), vec!["Doppelt".to_string()]);
}

#[test]
fn save_creates_the_directory_and_reads_back() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fancy-renamer").join("presets.json");
    let mut presets = Presets::default();
    presets.upsert(sample_preset("Alle")).unwrap();
    presets.save(&path).unwrap();
    assert!(path.exists());
    assert!(!path.with_extension("json.tmp").exists());
    assert_eq!(Presets::load(&path), presets);
}

#[test]
fn upsert_replaces_in_place_and_reports_it() {
    let mut presets = Presets::default();
    presets.upsert(sample_preset("Erste")).unwrap();
    presets.upsert(sample_preset("Zweite")).unwrap();
    let mut changed = sample_preset("Erste");
    changed.rules.clear();
    assert!(presets.upsert(changed).unwrap());
    assert_eq!(
        presets.names(),
        vec!["Erste".to_string(), "Zweite".to_string()]
    );
    assert!(presets.entries()[0].rules.is_empty());
    assert!(!presets.upsert(sample_preset("Dritte")).unwrap());
}

#[test]
fn rename_rejects_empty_and_taken_names() {
    let mut presets = Presets::default();
    presets.upsert(sample_preset("A")).unwrap();
    presets.upsert(sample_preset("B")).unwrap();
    assert!(presets.rename("A", "   ").is_err());
    assert!(presets.rename("A", "B").is_err());
    assert!(presets.rename("A", "A").unwrap());
    assert!(presets.rename("A", "C").unwrap());
    assert!(!presets.rename("Fehlt", "D").unwrap());
    assert_eq!(presets.names(), vec!["C".to_string(), "B".to_string()]);
}

#[test]
fn remove_reports_missing_names() {
    let mut presets = Presets::default();
    presets.upsert(sample_preset("A")).unwrap();
    assert!(presets.remove("A"));
    assert!(!presets.remove("A"));
    assert!(presets.is_empty());
}

#[test]
fn merge_counts_new_and_overwritten_presets() {
    let mut target = Presets::default();
    target.upsert(sample_preset("A")).unwrap();
    let mut source = Presets::default();
    source.upsert(sample_preset("A")).unwrap();
    source.upsert(sample_preset("B")).unwrap();
    let stats = target.merge(source);
    assert_eq!(
        stats,
        MergeStats {
            imported: 1,
            overwritten: 1
        }
    );
    assert_eq!(target.names(), vec!["A".to_string(), "B".to_string()]);
}

#[test]
fn names_are_trimmed_and_limited() {
    assert_eq!(validate_name("  Fotos  ").unwrap(), "Fotos");
    assert!(validate_name("   ").is_err());
    assert!(validate_name(&"x".repeat(81)).is_err());
}
