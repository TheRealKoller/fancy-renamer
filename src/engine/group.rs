use crate::engine::FileItem;

/// Dateien mit identischem Stamm bilden eine Gruppe (z. B. `abc.jpg` +
/// `abc.raw`). Die Gruppe ist die Einheit, auf die Regeln wirken: alle
/// Mitglieder erhalten denselben neuen Stamm und behalten ihre Erweiterung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGroup {
    /// Stamm, wie er beim Einlesen war.
    pub original_stem: String,
    pub members: Vec<FileItem>,
}

impl FileGroup {
    /// Dateigruppen aus einer Dateiliste bilden; die Reihenfolge der Gruppen und
    /// Mitglieder folgt der Eingabereihenfolge.
    pub fn from_items(items: &[FileItem]) -> Vec<Self> {
        let mut groups: Vec<Self> = Vec::new();
        for item in items {
            match groups
                .iter_mut()
                .find(|group| group.original_stem == item.stem)
            {
                Some(group) => group.members.push(item.clone()),
                None => groups.push(Self {
                    original_stem: item.stem.clone(),
                    members: vec![item.clone()],
                }),
            }
        }
        groups
    }

    pub fn is_pair(&self) -> bool {
        self.members.len() > 1
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::FileGroup;
    use crate::engine::FileItem;

    fn item(name: &str) -> FileItem {
        let path = PathBuf::from("/tmp").join(name);
        let (stem, extension) = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), ext.to_string()),
            _ => (name.to_string(), String::new()),
        };
        FileItem {
            path,
            stem,
            extension,
            size: 0,
            modified: None,
        }
    }

    #[test]
    fn groups_same_stem_with_different_extensions() {
        let groups = FileGroup::from_items(&[item("abc.jpg"), item("abc.raw"), item("def.jpg")]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].members.len(), 2);
        assert!(groups[0].is_pair());
        assert!(!groups[1].is_pair());
    }

    #[test]
    fn keeps_order_of_first_appearance() {
        let groups = FileGroup::from_items(&[item("b.jpg"), item("a.jpg"), item("b.raw")]);
        assert_eq!(groups[0].original_stem, "b");
        assert_eq!(groups[0].members.len(), 2);
        assert_eq!(groups[1].original_stem, "a");
    }

    #[test]
    fn grouping_is_case_sensitive() {
        let groups = FileGroup::from_items(&[item("Abc.jpg"), item("abc.jpg")]);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn dotfile_is_its_own_group() {
        let groups = FileGroup::from_items(&[item(".gitignore")]);
        assert_eq!(groups[0].original_stem, ".gitignore");
        assert_eq!(groups[0].members[0].extension, "");
    }
}
