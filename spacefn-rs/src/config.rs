use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub keyboard: String,
    pub keys_map: Vec<[u32; 3]>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keyboard: String::new(),
            keys_map: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_paths = Self::config_paths();

        for path in config_paths {
            if path.exists() {
                let content = std::fs::read_to_string(&path)?;
                let config: Config = toml::from_str(&content)?;
                log::info!("Loaded config from {:?}", path);
                return Ok(config);
            }
        }

        log::warn!("No config file found, using default config");
        Ok(Config::default())
    }

    fn config_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".config/spacefn/config.toml"));
        }

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                paths.push(exe_dir.join("configs/default.toml"));
            }
        }

        paths.push(PathBuf::from("/etc/spacefn/config.toml"));

        paths
    }

    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        log::info!("Saved config to {:?}", path);
        Ok(())
    }
}
