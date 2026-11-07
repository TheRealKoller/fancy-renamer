use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Eine vom Nutzer ausgewählte Datei samt der für die Vorschau relevanten Metadaten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileItem {
    pub path: PathBuf,
    /// Dateiname ohne letzte Erweiterung; `.gitignore` hat den Stamm `.gitignore`.
    pub stem: String,
    /// Erweiterung ohne Punkt; leer, wenn der Name keine hat.
    pub extension: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

impl FileItem {
    pub fn from_path(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let metadata = fs::metadata(&path)?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Dateiname ist kein UTF-8")
            })?;
        let (stem, extension) = split_name(file_name);
        Ok(Self {
            path,
            stem,
            extension,
            size: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }

    /// Dateiname für einen neuen Stamm; die Erweiterung bleibt erhalten.
    pub fn file_name_for(&self, stem: &str) -> String {
        if self.extension.is_empty() {
            stem.to_string()
        } else {
            format!("{stem}.{}", self.extension)
        }
    }

    pub fn target_path(&self, stem: &str) -> PathBuf {
        let mut path = self.path.clone();
        path.set_file_name(self.file_name_for(stem));
        path
    }

    pub fn directory(&self) -> &Path {
        self.path.parent().unwrap_or_else(|| Path::new("."))
    }

    /// Wurde die Datei seit dem Einlesen verändert?
    pub fn is_unchanged_on_disk(&self) -> bool {
        fs::metadata(&self.path)
            .is_ok_and(|meta| meta.len() == self.size && meta.modified().ok() == self.modified)
    }
}

/// Zerlegt einen Dateinamen in Stamm und Erweiterung.
fn split_name(file_name: &str) -> (String, String) {
    match file_name.rsplit_once('.') {
        // Führender Punkt zählt nicht als Erweiterung (`.gitignore`).
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), ext.to_string()),
        _ => (file_name.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::split_name;

    #[test]
    fn splits_regular_name() {
        assert_eq!(split_name("foto.jpg"), ("foto".into(), "jpg".into()));
    }

    #[test]
    fn splits_last_extension_only() {
        assert_eq!(
            split_name("archiv.tar.gz"),
            ("archiv.tar".into(), "gz".into())
        );
    }

    #[test]
    fn dotfile_has_no_extension() {
        assert_eq!(
            split_name(".gitignore"),
            (".gitignore".into(), String::new())
        );
    }

    #[test]
    fn name_without_dot_has_no_extension() {
        assert_eq!(split_name("README"), ("README".into(), String::new()));
    }

    #[test]
    fn trailing_dot_is_part_of_stem() {
        assert_eq!(split_name("name."), ("name".into(), String::new()));
    }
}
