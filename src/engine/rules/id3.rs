use std::path::Path;

use id3::TagLike;
use serde::{Deserialize, Serialize};

use crate::engine::{Rule, RuleContext, RuleError, RuleOutcome};

/// Ersetzt den Stamm durch die ID3-Angaben oder stellt sie voran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Id3Mode {
    Replace,
    Prefix,
}

/// Platzhalter, die aus den ID3-Tags gefüllt werden.
const PLACEHOLDERS: [&str; 5] = ["artist", "title", "album", "track", "year"];

const DEFAULT_FORMAT: &str = "{artist} - {title}";

/// Dateiname aus ID3-Tags: `{artist}`, `{title}`, `{album}`, `{track}`
/// (zweistellig) und `{year}`.
///
/// Gelesen wird ID3v2; fehlt der Tag, dient ID3v1 als Rückfall (latin-1
/// dekodiert die Bibliothek). Ohne lesbare Tags überspringt die Regel die
/// Datei wie die EXIF-Regel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Id3TagRule {
    format: String,
    mode: Id3Mode,
}

impl Id3TagRule {
    /// `format` darf nur die bekannten Platzhalter enthalten; unbekannte
    /// Namen sind ein Fehler im Regelaufbau.
    pub fn new(format: impl Into<String>, mode: Id3Mode) -> Result<Self, RuleError> {
        let format = format.into();
        if let Some(name) = unknown_placeholder(&format) {
            return Err(RuleError::InvalidPattern(format!(
                "unbekannter Platzhalter: {{{name}}}"
            )));
        }
        Ok(Self { format, mode })
    }

    pub fn with_defaults(mode: Id3Mode) -> Self {
        Self {
            format: DEFAULT_FORMAT.to_string(),
            mode,
        }
    }

    pub fn format(&self) -> &str {
        &self.format
    }

    pub fn mode(&self) -> Id3Mode {
        self.mode
    }
}

impl Rule for Id3TagRule {
    fn id(&self) -> &'static str {
        "id3_tags"
    }

    fn apply(&self, stem: &str, ctx: &RuleContext<'_>) -> Result<RuleOutcome, RuleError> {
        let Some(tags) = read_tags(ctx.path) else {
            return Ok(RuleOutcome::Skip("keine ID3-Daten"));
        };
        let filled = fill(&self.format, &tags);
        let value = tidy(&filled);
        if value.is_empty() {
            return Ok(RuleOutcome::Skip("keine ID3-Daten"));
        }
        Ok(RuleOutcome::Stem(match self.mode {
            Id3Mode::Replace => value,
            Id3Mode::Prefix => format!("{value}_{stem}"),
        }))
    }
}

/// Werte der unterstützten Tags; fehlende Angaben bleiben `None`.
#[derive(Debug, Default)]
struct Tags {
    artist: Option<String>,
    title: Option<String>,
    album: Option<String>,
    track: Option<u32>,
    year: Option<i32>,
}

impl Tags {
    fn value(&self, placeholder: &str) -> String {
        match placeholder {
            "artist" => self.artist.clone().unwrap_or_default(),
            "title" => self.title.clone().unwrap_or_default(),
            "album" => self.album.clone().unwrap_or_default(),
            "track" => self
                .track
                .map(|track| format!("{track:02}"))
                .unwrap_or_default(),
            "year" => self.year.map(|year| year.to_string()).unwrap_or_default(),
            _ => String::new(),
        }
    }
}

/// ID3v2 zuerst, ID3v1 als Rückfall; `None`, wenn die Datei keine lesbaren
/// Tags enthält.
fn read_tags(path: &Path) -> Option<Tags> {
    let tag = ::id3::v1v2::read_from_path(path).ok()?;
    let tags = Tags {
        artist: tag.artist().map(str::to_string),
        title: tag.title().map(str::to_string),
        album: tag.album().map(str::to_string),
        track: tag.track(),
        year: tag.year(),
    };
    let empty = tags.artist.is_none()
        && tags.title.is_none()
        && tags.album.is_none()
        && tags.track.is_none()
        && tags.year.is_none();
    (!empty).then_some(tags)
}

/// Erster Platzhalter im Muster, der nicht unterstützt wird.
fn unknown_placeholder(format: &str) -> Option<&str> {
    let mut rest = format;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            break;
        };
        let name = &after[..end];
        if !PLACEHOLDERS.contains(&name) {
            return Some(name);
        }
        rest = &after[end + 1..];
    }
    None
}

fn fill(format: &str, tags: &Tags) -> String {
    let mut out = String::with_capacity(format.len());
    let mut rest = format;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        out.push_str(&tags.value(&after[..end]));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Leere Platzhalter hinterlassen doppelte Trennzeichen und Leerzeichen;
/// Normalform ist ein einzelnes Leerzeichen zwischen Wörtern und ein einzelnes
/// Trennzeichen aus `-`, `_`, `,`, `.`, ohne Rand.
fn tidy(text: &str) -> String {
    let mut spaced = String::with_capacity(text.len());
    let mut last_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_space && !spaced.is_empty() {
                spaced.push(' ');
            }
            last_space = true;
        } else {
            spaced.push(ch);
            last_space = false;
        }
    }

    let mut out = String::with_capacity(spaced.len());
    let mut previous_separator = None;
    for ch in spaced.chars() {
        if matches!(ch, ' ' | '-' | '_' | ',' | '.') {
            if previous_separator == Some(ch) {
                continue;
            }
            previous_separator = Some(ch);
        } else {
            previous_separator = None;
        }
        out.push(ch);
    }

    // „A - - B“ (leerer Platzhalter zwischen Trennern) auf „A - B“ bringen.
    while out.contains(" - - ") {
        out = out.replace(" - - ", " - ");
    }
    out.trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '-' | '_' | ',' | '.'))
        .to_string()
}
