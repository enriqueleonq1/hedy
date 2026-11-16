use std::path::{Path, PathBuf};

use crate::application::ports::{PortError, PortResult};
use crate::tr;

/// Where Hedy keeps its data. `HEDY_DATA_DIR` overrides the XDG location
/// (`~/.local/share/hedy` on Linux).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HedyPaths {
    data_dir: PathBuf,
}

impl HedyPaths {
    pub const DATA_DIR_VAR: &'static str = "HEDY_DATA_DIR";

    pub fn detect() -> PortResult<Self> {
        if let Some(dir) = std::env::var_os(Self::DATA_DIR_VAR).filter(|d| !d.is_empty()) {
            return Ok(Self::at(dir.into()));
        }
        let dirs = directories::ProjectDirs::from("", "", "hedy").ok_or_else(|| {
            PortError::new(tr!(
                "no se encontró la carpeta personal del usuario",
                "the user's home folder was not found"
            ))
        })?;
        Ok(Self::at(dirs.data_dir().to_owned()))
    }

    pub fn at(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    pub fn database(&self) -> PathBuf {
        self.data_dir.join("hedy.db")
    }

    pub fn accounts_dir(&self) -> PathBuf {
        self.data_dir.join("accounts")
    }

    /// Private scratch folders of conversations outside any project.
    pub fn chats_dir(&self) -> PathBuf {
        self.data_dir.join("chats")
    }

    /// Custom themes, one JSON file each.
    pub fn themes_dir(&self) -> PathBuf {
        self.data_dir.join("themes")
    }
}

/// `path` relative to `base` for display (`src/app.py`), or as given when it
/// is elsewhere.
pub(crate) fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
