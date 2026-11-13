use std::path::PathBuf;

use crate::application::ports::{PortError, PortResult};

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
        let dirs = directories::ProjectDirs::from("", "", "hedy")
            .ok_or_else(|| PortError::new("no se encontró la carpeta personal del usuario"))?;
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
}
