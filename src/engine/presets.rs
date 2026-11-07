//! Benannte Regelketten: speichern, laden, umbenennen, löschen, austauschen.
//!
//! Eine Datei (`presets.json`) hält alle Presets. Sie enthält ausschließlich
//! Regelbeschreibungen und niemals Dateipfade. Fehlende oder defekte Dateien
//! ergeben eine leere Liste, damit ein kaputter Eintrag die Oberfläche nicht
//! blockiert.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::engine::{
    CaseMode, ExifDateRule, ExifMode, Id3Mode, Id3TagRule, MatchMode, NormalizeOptions,
    NormalizeRule, NumberingMode, NumberingRule, ReplaceRule, Rule,
};

/// Dateiname innerhalb des Konfigurationsverzeichnisses.
pub const PRESET_FILE: &str = "presets.json";

/// Aktuelles Format der Datei; ältere Dateien ohne Versionsangabe gelten als 1.
const FORMAT_VERSION: u32 = 1;

/// Name eines Presets: getrimmt, nicht leer, höchstens 80 Zeichen.
pub fn validate_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Preset braucht einen Namen".into());
    }
    if trimmed.chars().count() > 80 {
        return Err("Name ist länger als 80 Zeichen".into());
    }
    Ok(trimmed.to_string())
}

/// Eine Regel samt aller Parameter, so wie sie in der Oberfläche eingestellt ist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum RulePreset {
    Replace {
        mode: MatchMode,
        pattern: String,
        replacement: String,
        case_insensitive: bool,
        first_only: bool,
    },
    Numbering {
        mode: NumberingMode,
        start: u64,
        step: u64,
        digits: usize,
        prefix: String,
        suffix: String,
    },
    Normalize {
        case: CaseMode,
        spaces_to_underscore: bool,
        trim_separators: bool,
        transliterate: bool,
        strip_special: bool,
    },
    ExifDate {
        format: String,
        mode: ExifMode,
    },
    Id3Tags {
        format: String,
        mode: Id3Mode,
    },
}

impl RulePreset {
    /// Kennung der Regelart, passend zu [`Rule::id`].
    pub fn rule_id(&self) -> &'static str {
        match self {
            Self::Replace { .. } => "replace",
            Self::Numbering { .. } => "numbering",
            Self::Normalize { .. } => "normalize",
            Self::ExifDate { .. } => "exif_date",
            Self::Id3Tags { .. } => "id3_tags",
        }
    }

    /// Baut die Regel neu; ungültige Parameter sind ein Fehler wie in der Oberfläche.
    pub fn build(&self) -> Result<Box<dyn Rule>, String> {
        match self {
            Self::Replace {
                mode,
                pattern,
                replacement,
                case_insensitive,
                first_only,
            } => ReplaceRule::new(
                *mode,
                pattern.clone(),
                replacement.clone(),
                *case_insensitive,
                *first_only,
            )
            .map(|rule| Box::new(rule) as Box<dyn Rule>)
            .map_err(|error| error.to_string()),
            Self::Numbering {
                mode,
                start,
                step,
                digits,
                prefix,
                suffix,
            } => NumberingRule::new(
                *mode,
                *start,
                *step,
                *digits,
                prefix.clone(),
                suffix.clone(),
            )
            .map(|rule| Box::new(rule) as Box<dyn Rule>)
            .map_err(|error| error.to_string()),
            Self::Normalize {
                case,
                spaces_to_underscore,
                trim_separators,
                transliterate,
                strip_special,
            } => Ok(Box::new(NormalizeRule::new(NormalizeOptions {
                case: *case,
                spaces_to_underscore: *spaces_to_underscore,
                trim_separators: *trim_separators,
                transliterate: *transliterate,
                strip_special: *strip_special,
            }))),
            Self::ExifDate { format, mode } => ExifDateRule::new(format.clone(), *mode)
                .map(|rule| Box::new(rule) as Box<dyn Rule>)
                .map_err(|error| error.to_string()),
            Self::Id3Tags { format, mode } => Id3TagRule::new(format.clone(), *mode)
                .map(|rule| Box::new(rule) as Box<dyn Rule>)
                .map_err(|error| error.to_string()),
        }
    }
}

/// Ein benanntes Preset: Name plus aktive Regeln in Anwendungsreihenfolge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub rules: Vec<RulePreset>,
}

impl Preset {
    /// Baut die vollständige Kette; der erste fehlerhafte Parameter bricht ab.
    pub fn build_chain(&self) -> Result<crate::engine::RuleChain, String> {
        let rules = self
            .rules
            .iter()
            .map(RulePreset::build)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(crate::engine::RuleChain::new(rules))
    }
}

/// Aufbau der Datei; `version` erlaubt spätere Migrationen.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    presets: Vec<Preset>,
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

/// Alle Presets in Dateireihenfolge.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Presets {
    entries: Vec<Preset>,
}

/// Ergebnis eines Imports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeStats {
    /// Neu aufgenommene Presets.
    pub imported: usize,
    /// Presets, die einen vorhandenen Namen ersetzt haben.
    pub overwritten: usize,
}

impl Presets {
    /// Liest die Datei; fehlt sie oder ist sie defekt, ist das Ergebnis leer.
    /// Einträge ohne gültigen Namen und doppelte Namen fallen still heraus.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        Self::from_json(&text).unwrap_or_default()
    }

    /// Parst JSON; ein Syntaxfehler ist ein Fehler, unbrauchbare Presets
    /// werden aussortiert.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let file: PresetFile =
            serde_json::from_str(text).map_err(|error| format!("Ungültiges JSON: {error}"))?;
        if file.version > FORMAT_VERSION {
            return Err(format!(
                "Datei stammt aus einer neueren Version (Format {})",
                file.version
            ));
        }
        let mut entries: Vec<Preset> = Vec::new();
        for preset in file.presets {
            let Ok(name) = validate_name(&preset.name) else {
                continue;
            };
            if entries.iter().any(|existing| existing.name == name) {
                continue;
            }
            entries.push(Preset {
                name,
                rules: preset.rules,
            });
        }
        Ok(Self { entries })
    }

    /// Serialisiert alle Presets als hübsch formatiertes JSON.
    pub fn to_json(&self) -> Result<String, String> {
        let file = PresetFile {
            version: FORMAT_VERSION,
            presets: self.entries.clone(),
        };
        serde_json::to_string_pretty(&file).map_err(|error| error.to_string())
    }

    /// Schreibt atomar: erst eine Temporärdatei, dann umbenennen.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = self.to_json().map_err(io::Error::other)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, path)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn entries(&self) -> &[Preset] {
        &self.entries
    }

    pub fn names(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|preset| preset.name.clone())
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<&Preset> {
        self.entries.iter().find(|preset| preset.name == name)
    }

    /// Nimmt ein Preset auf; ein vorhandener Name wird ersetzt (Position bleibt).
    /// Rückgabe: `true`, wenn ein vorhandenes Preset ersetzt wurde.
    pub fn upsert(&mut self, preset: Preset) -> Result<bool, String> {
        let name = validate_name(&preset.name)?;
        let preset = Preset {
            name,
            rules: preset.rules,
        };
        match self
            .entries
            .iter_mut()
            .find(|existing| existing.name == preset.name)
        {
            Some(existing) => {
                *existing = preset;
                Ok(true)
            }
            None => {
                self.entries.push(preset);
                Ok(false)
            }
        }
    }

    /// Entfernt ein Preset; `false`, wenn der Name nicht existiert.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|preset| preset.name != name);
        self.entries.len() != before
    }

    /// Benennt ein Preset um; leerer Name und vergebene Namen sind Fehler.
    /// Rückgabe: `false`, wenn das alte Preset fehlt.
    pub fn rename(&mut self, old: &str, new: &str) -> Result<bool, String> {
        let new = validate_name(new)?;
        if new != old && self.entries.iter().any(|preset| preset.name == new) {
            return Err(format!("Es gibt bereits ein Preset „{new}“"));
        }
        let Some(preset) = self.entries.iter_mut().find(|preset| preset.name == old) else {
            return Ok(false);
        };
        preset.name = new;
        Ok(true)
    }

    /// Führt Presets aus einer Importdatei zusammen; gleiche Namen überschreiben.
    pub fn merge(&mut self, other: Presets) -> MergeStats {
        let mut stats = MergeStats::default();
        for preset in other.entries {
            match self.upsert(preset) {
                Ok(true) => stats.overwritten += 1,
                Ok(false) => stats.imported += 1,
                Err(_) => {}
            }
        }
        stats
    }
}
