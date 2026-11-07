use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use chrono::NaiveDateTime;

use crate::engine::{Rule, RuleContext, RuleError, RuleOutcome};

/// Ersetzt den Stamm durch das Aufnahmedatum oder stellt es voran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExifMode {
    Replace,
    Prefix,
}

/// Dateiname aus dem EXIF-Aufnahmedatum (`DateTimeOriginal`, Fallback
/// `DateTimeDigitized`, dann `DateTime`).
#[derive(Debug, Clone)]
pub struct ExifDateRule {
    format: String,
    mode: ExifMode,
}

impl ExifDateRule {
    /// `format` ist ein chrono-Formatmuster, z. B. `%Y-%m-%d_%H-%M-%S`.
    pub fn new(format: impl Into<String>, mode: ExifMode) -> Result<Self, RuleError> {
        let format = format.into();
        if chrono::format::StrftimeItems::new(&format)
            .any(|item| matches!(item, chrono::format::Item::Error))
        {
            return Err(RuleError::InvalidPattern(format!(
                "ungültiges Datumsformat: {format}"
            )));
        }
        Ok(Self { format, mode })
    }

    pub fn with_defaults(mode: ExifMode) -> Self {
        Self {
            format: "%Y-%m-%d_%H-%M-%S".to_string(),
            mode,
        }
    }

    pub fn format(&self) -> &str {
        &self.format
    }

    pub fn mode(&self) -> ExifMode {
        self.mode
    }
}

impl Rule for ExifDateRule {
    fn id(&self) -> &'static str {
        "exif_date"
    }

    fn apply(&self, stem: &str, ctx: &RuleContext<'_>) -> Result<RuleOutcome, RuleError> {
        let Some(date) = read_capture_date(ctx.path) else {
            return Ok(RuleOutcome::Skip("keine EXIF-Daten"));
        };
        let formatted = date.format(&self.format).to_string();
        Ok(RuleOutcome::Stem(match self.mode {
            ExifMode::Replace => formatted,
            ExifMode::Prefix => format!("{formatted}_{stem}"),
        }))
    }
}

/// Aufnahmezeitpunkt lesen; `None`, wenn keine passenden Metadaten lesbar sind.
fn read_capture_date(path: &Path) -> Option<NaiveDateTime> {
    let file = File::open(path).ok()?;
    let mut reader = exif::Reader::new();
    let exif = reader.read_from_container(&mut BufReader::new(file)).ok()?;
    let tags = [
        exif::Tag::DateTimeOriginal,
        exif::Tag::DateTimeDigitized,
        exif::Tag::DateTime,
    ];
    tags.iter()
        .find_map(|tag| exif.get_field(*tag, exif::In::PRIMARY))
        .and_then(parse_datetime)
}

/// EXIF speichert Zeitstempel als ASCII im Format `YYYY:MM:DD HH:MM:SS`.
fn parse_datetime(field: &exif::Field) -> Option<NaiveDateTime> {
    let raw = match &field.value {
        exif::Value::Ascii(values) => std::str::from_utf8(values.first()?).ok()?,
        _ => return None,
    };
    let text = raw.trim_end_matches('\0').trim();
    NaiveDateTime::parse_from_str(text, "%Y:%m:%d %H:%M:%S").ok()
}
