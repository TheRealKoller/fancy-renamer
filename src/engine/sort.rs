//! Natürliche Sortierung: Ziffernfolgen werden als Zahlen verglichen, damit
//! `foto2` vor `foto10` steht. Groß-/Kleinschreibung spielt keine Rolle.

use std::cmp::Ordering;

use crate::engine::FileItem;

/// Bestandteil eines Sortierschlüssels.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum KeyPart {
    /// Textstück in Kleinschreibung.
    Text(String),
    /// Ziffernfolge als Zahl.
    Number(u64),
}

/// Sortierschlüssel eines Namens.
fn key(name: &str) -> Vec<KeyPart> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut digits = String::new();

    for character in name.chars() {
        if character.is_ascii_digit() {
            if !text.is_empty() {
                parts.push(KeyPart::Text(std::mem::take(&mut text)));
            }
            digits.push(character);
        } else {
            if !digits.is_empty() {
                parts.push(KeyPart::Number(digits.parse().unwrap_or(u64::MAX)));
                digits.clear();
            }
            text.extend(character.to_lowercase());
        }
    }
    if !digits.is_empty() {
        parts.push(KeyPart::Number(digits.parse().unwrap_or(u64::MAX)));
    }
    if !text.is_empty() {
        parts.push(KeyPart::Text(text));
    }
    parts
}

/// Natürlicher Vergleich zweier Namen. Gleiche Schlüssel (etwa `01` und `1`
/// oder `Bild` und `bild`) gelten als gleich; die Liste bleibt dann stabil.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    key(a).cmp(&key(b))
}

/// Dateien nach ihrem Namen natürlich sortieren.
pub fn sort_items(items: &mut [FileItem]) {
    items.sort_by(|a, b| natural_cmp(&a.file_name_for(&a.stem), &b.file_name_for(&b.stem)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_numbers_numerically() {
        assert_eq!(natural_cmp("foto2.jpg", "foto10.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("foto10.jpg", "foto9.jpg"), Ordering::Greater);
        assert_eq!(natural_cmp("foto10.jpg", "foto10.jpg"), Ordering::Equal);
    }

    #[test]
    fn ignores_case() {
        assert_eq!(natural_cmp("Bild.jpg", "bild.jpg"), Ordering::Equal);
    }

    #[test]
    fn sorts_text_before_numbers_at_the_same_position() {
        assert_eq!(natural_cmp("bild1.jpg", "bilda.jpg"), Ordering::Less);
    }
}
