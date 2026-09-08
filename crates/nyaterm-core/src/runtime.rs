use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("unable to resolve home directory for installed configuration")]
    MissingConfigDir,
    #[error("failed to prepare runtime directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMode {
    Portable,
    Installed,
}

#[derive(Debug, Clone)]
pub struct AppRuntime {
    mode: RuntimeMode,
    data_dir: PathBuf,
    config_dir: PathBuf,
    log_dir: PathBuf,
    cache_dir: PathBuf,
    portable_key_path: Option<PathBuf>,
}

impl AppRuntime {
    pub fn resolve() -> Result<Self, RuntimeError> {
        if let Some(runtime) = Self::portable_from_current_exe() {
            return Ok(runtime);
        }

        let home = dirs::home_dir().ok_or(RuntimeError::MissingConfigDir)?;
        let config_dir = home.join(".nyaterm");
        let data_dir = config_dir.clone();
        let log_dir = data_dir.join("logs");
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| data_dir.join("cache"))
            .join("nyaterm");

        Ok(Self {
            mode: RuntimeMode::Installed,
            data_dir,
            config_dir,
            log_dir,
            cache_dir,
            portable_key_path: None,
        })
    }

    pub fn ensure_directories(&self) -> Result<(), RuntimeError> {
        for path in [
            self.data_dir(),
            self.config_dir(),
            self.log_dir(),
            self.cache_dir(),
        ] {
            std::fs::create_dir_all(path).map_err(|source| RuntimeError::CreateDir {
                path: path.to_path_buf(),
                source,
            })?;
        }
        Ok(())
    }

    pub fn mode(&self) -> RuntimeMode {
        self.mode
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn portable_key_path(&self) -> Option<&Path> {
        self.portable_key_path.as_deref()
    }

    fn portable_from_current_exe() -> Option<Self> {
        let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
        let marker = exe_dir.join("nyaterm-portable");
        if !marker.exists() {
            return None;
        }

        let data_dir = exe_dir.join("data");
        Some(Self {
            mode: RuntimeMode::Portable,
            config_dir: data_dir.join("config"),
            log_dir: data_dir.join("logs"),
            cache_dir: data_dir.join("cache"),
            portable_key_path: Some(data_dir.join("config").join("portable.key")),
            data_dir,
        })
    }

    #[doc(hidden)]
    pub fn from_parts_for_test(
        mode: RuntimeMode,
        data_dir: PathBuf,
        config_dir: PathBuf,
        log_dir: PathBuf,
        cache_dir: PathBuf,
        portable_key_path: Option<PathBuf>,
    ) -> Self {
        Self {
            mode,
            data_dir,
            config_dir,
            log_dir,
            cache_dir,
            portable_key_path,
        }
    }
}
