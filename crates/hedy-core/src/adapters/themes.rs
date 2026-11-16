//! Custom themes as JSON files, one per theme (`<data dir>/themes/<id>.json`),
//! pretty-printed so users can share them or drop one in. The file name is
//! the theme's id.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::application::ports::{PortError, PortResult, ThemeStore};
use crate::domain::theme::{self, CustomTheme, ThemeBase, ThemeOrigin};
use crate::tr;

pub struct JsonThemeStore {
    dir: PathBuf,
}

impl JsonThemeStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

/// What a theme file holds. A dropped-in file may lack its id (the file
/// name is used) or its origin.
#[derive(Deserialize)]
struct ThemeFile {
    name: String,
    base: ThemeBase,
    colors: BTreeMap<String, String>,
    #[serde(default)]
    origin: Option<ThemeOrigin>,
}

fn io_error(path: &Path, err: impl std::fmt::Display) -> PortError {
    let path = path.display();
    PortError::new(tr!(
        "no se pudo usar el tema {path}: {err}",
        "could not use theme {path}: {err}"
    ))
}

/// Reads and checks one theme file; `None` when it does not exist.
fn read_theme(path: &Path, id: &str) -> PortResult<Option<CustomTheme>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(path, e)),
    };
    let file: ThemeFile = serde_json::from_str(&text).map_err(|e| io_error(path, e))?;
    CustomTheme::new(
        id,
        &file.name,
        file.base,
        file.colors,
        file.origin.unwrap_or(ThemeOrigin::Edited),
    )
    .map(Some)
    .map_err(|e| io_error(path, e))
}

impl ThemeStore for JsonThemeStore {
    fn list(&self) -> PortResult<Vec<CustomTheme>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_error(&self.dir, e)),
        };
        let mut themes = Vec::new();
        for path in entries.filter_map(|e| e.ok()).map(|e| e.path()) {
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if !theme::is_theme_id(id) {
                eprintln!(
                    "hedy: skipping theme {}: its file name is not a theme id",
                    path.display()
                );
                continue;
            }
            match read_theme(&path, id) {
                Ok(Some(theme)) => themes.push(theme),
                Ok(None) => {}
                Err(e) => eprintln!("hedy: skipping theme: {e}"),
            }
        }
        Ok(themes)
    }

    fn get(&self, id: &str) -> PortResult<Option<CustomTheme>> {
        if !theme::is_theme_id(id) {
            return Ok(None);
        }
        read_theme(&self.path(id), id)
    }

    fn save(&self, theme: &CustomTheme) -> PortResult<()> {
        if !theme::is_theme_id(&theme.id) {
            return Err(PortError::new(
                crate::domain::DomainError::InvalidThemeId(theme.id.clone()).to_string(),
            ));
        }
        fs::create_dir_all(&self.dir).map_err(|e| io_error(&self.dir, e))?;
        let path = self.path(&theme.id);
        let mut json = serde_json::to_string_pretty(theme).map_err(|e| io_error(&path, e))?;
        json.push('\n');
        // Written aside, then renamed: a crash never leaves half a theme.
        let temp = self.dir.join(format!(".{}.json.tmp", theme.id));
        fs::write(&temp, json).map_err(|e| io_error(&temp, e))?;
        fs::rename(&temp, &path).map_err(|e| io_error(&path, e))
    }

    fn delete(&self, id: &str) -> PortResult<()> {
        if !theme::is_theme_id(id) {
            return Ok(());
        }
        let path = self.path(id);
        match fs::remove_file(&path) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(io_error(&path, e)),
            _ => Ok(()),
        }
    }

    fn work_dir(&self) -> PortResult<PathBuf> {
        fs::create_dir_all(&self.dir).map_err(|e| io_error(&self.dir, e))?;
        Ok(self.dir.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(id: &str, name: &str) -> CustomTheme {
        CustomTheme::new(
            id,
            name,
            ThemeBase::Dark,
            [("bg".to_owned(), "#1e1e2e".to_owned())].into(),
            ThemeOrigin::Described,
        )
        .unwrap()
    }

    #[test]
    fn themes_round_trip_as_pretty_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonThemeStore::new(dir.path().join("themes"));
        assert!(store.list().unwrap().is_empty());
        assert_eq!(store.get("noche").unwrap(), None);

        store.save(&theme("noche", "Noche")).unwrap();
        let text = fs::read_to_string(dir.path().join("themes/noche.json")).unwrap();
        assert!(text.contains("\n  \"name\": \"Noche\""), "{text}");
        assert_eq!(store.get("noche").unwrap(), Some(theme("noche", "Noche")));

        store.save(&theme("noche", "Noche 2")).unwrap();
        assert_eq!(store.list().unwrap(), vec![theme("noche", "Noche 2")]);

        store.delete("noche").unwrap();
        store.delete("noche").unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn broken_files_are_skipped_and_dropped_files_read() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonThemeStore::new(dir.path().to_owned());
        store.save(&theme("noche", "Noche")).unwrap();
        fs::write(dir.path().join("roto.json"), "{ not json").unwrap();
        fs::write(
            dir.path().join("malo.json"),
            r##"{"name": "Malo", "base": "dark", "colors": {"bg": "negro"}}"##,
        )
        .unwrap();
        fs::write(dir.path().join("Mal Nombre.json"), "{}").unwrap();
        fs::write(dir.path().join("notas.txt"), "hola").unwrap();
        // Shared by someone else: no id or origin, another id inside.
        fs::write(
            dir.path().join("compartido.json"),
            r##"{"id": "otro", "name": " Compartido ", "base": "light", "colors": {"bg": "#fff"}}"##,
        )
        .unwrap();

        let mut themes = store.list().unwrap();
        themes.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(themes.len(), 2);
        assert_eq!(themes[0].id, "compartido");
        assert_eq!(themes[0].name, "Compartido");
        assert_eq!(themes[0].origin, ThemeOrigin::Edited);
        assert_eq!(themes[0].colors["bg"], "#FFFFFF");
        assert_eq!(themes[1], theme("noche", "Noche"));
        assert!(store.get("malo").is_err());
        assert_eq!(store.get("../noche").unwrap(), None);
    }
}
