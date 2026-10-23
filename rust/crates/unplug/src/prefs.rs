// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! What the player chose and the tool remembers: the category a device was moved to. One line per device: `046D:C215=keyboard`.

use crate::device::Category;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct Prefs {
    path: Option<PathBuf>,
    categories: HashMap<String, Category>,
}

impl Prefs {
    /// Reads the file (a missing or damaged one is an empty list); changes are written back to it.
    pub fn load(path: PathBuf) -> Self {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let categories = text
            .lines()
            .filter_map(|l| {
                let (key, cat) = l.split_once('=')?;
                Some((key.trim().to_string(), Category::from_key(cat.trim())?))
            })
            .collect();
        Prefs {
            path: Some(path),
            categories,
        }
    }

    /// Nothing is written: for the demo.
    pub fn in_memory() -> Self {
        Prefs::default()
    }

    /// `%APPDATA%\Unplug\categories.txt`.
    pub fn default_path() -> Option<PathBuf> {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("Unplug").join("categories.txt"))
    }

    pub fn category(&self, key: &str) -> Option<Category> {
        self.categories.get(key).copied()
    }

    /// Moves a device to a category, or back to the automatic one with `None`.
    pub fn set_category(&mut self, key: &str, category: Option<Category>) {
        match category {
            Some(c) => self.categories.insert(key.to_string(), c),
            None => self.categories.remove(key),
        };
        self.save();
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        let mut lines: Vec<String> = self.categories.iter().map(|(k, c)| format!("{k}={}", c.key())).collect();
        lines.sort();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, lines.join("\n") + "\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("unplug-prefs-{}", std::process::id()));
        let path = dir.join("categories.txt");
        let mut p = Prefs::load(path.clone());
        assert_eq!(p.category("1B1C:0001"), None);
        p.set_category("1B1C:0001", Some(Category::Keyboard));
        p.set_category("046D:C215", Some(Category::Controller));
        p.set_category("046D:C215", None);
        let again = Prefs::load(path);
        assert_eq!(again.category("1B1C:0001"), Some(Category::Keyboard));
        assert_eq!(again.category("046D:C215"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_file_is_an_empty_list() {
        let dir = std::env::temp_dir().join(format!("unplug-prefs-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("categories.txt");
        std::fs::write(&path, "garbage\nA:B=nonsense\nC:D=audio\n").unwrap();
        let p = Prefs::load(path);
        assert_eq!(p.category("A:B"), None);
        assert_eq!(p.category("C:D"), Some(Category::Audio));
        let _ = std::fs::remove_dir_all(dir);
    }
}
