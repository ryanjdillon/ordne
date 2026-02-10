use crate::error::{PruneError, Result};
use std::path::PathBuf;
use xdg::BaseDirectories;

pub struct Config {
    pub db_path: PathBuf,
    pub config_path: Option<PathBuf>,
}

impl Config {
    pub fn new(db_override: Option<PathBuf>) -> Result<Self> {
        let db_path = if let Some(path) = db_override {
            path
        } else if let Ok(env_path) = std::env::var("PRUNE_DB") {
            PathBuf::from(env_path)
        } else {
            let xdg = BaseDirectories::with_prefix("prune")
                .map_err(|e| PruneError::Config(format!("Failed to initialize XDG directories: {}", e)))?;
            xdg.place_data_file("prune.db")
                .map_err(|e| PruneError::Config(format!("Failed to create data directory: {}", e)))?
        };

        let config_path = BaseDirectories::with_prefix("prune")
            .ok()
            .and_then(|xdg| xdg.find_config_file("prune.toml"));

        Ok(Self {
            db_path,
            config_path,
        })
    }

    pub fn ensure_db_directory(&self) -> Result<()> {
        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_with_override() {
        let custom_path = PathBuf::from("/tmp/test.db");
        let config = Config::new(Some(custom_path.clone())).unwrap();
        assert_eq!(config.db_path, custom_path);
    }

    #[test]
    fn test_config_ensure_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("subdir/test.db");
        let config = Config::new(Some(db_path.clone())).unwrap();
        config.ensure_db_directory().unwrap();
        assert!(db_path.parent().unwrap().exists());
    }
}
