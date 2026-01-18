use std::path::PathBuf;

use serde::Deserialize;

use crate::error::SihError;
use crate::store::{default_store_path, StoreType};

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub openrouter_api_key: Option<String>,
    #[serde(default)]
    pub store: StoreConfig,
}

#[derive(Debug, Deserialize)]
pub struct StoreConfig {
    #[serde(default)]
    pub r#type: StoreType,
    pub path: Option<PathBuf>,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            r#type: StoreType::default(),
            path: None,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("silane").join("config.toml"))
}

pub fn load_config() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };

    let Ok(content) = std::fs::read_to_string(path) else {
        return Config::default();
    };

    toml::from_str(&content).unwrap_or_default()
}

pub fn load_api_key() -> Result<String, SihError> {
    // First, try environment variable
    if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    // Then, try config file
    let config = load_config();
    if let Some(key) = config.openrouter_api_key {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    Err(SihError::ApiKeyNotFound)
}

pub fn resolve_store_config(
    cli_type: Option<StoreType>,
    cli_path: Option<PathBuf>,
) -> (StoreType, PathBuf) {
    let config = load_config();

    let store_type = cli_type.unwrap_or(config.store.r#type);
    let store_path = cli_path
        .or(config.store.path)
        .unwrap_or_else(default_store_path);

    (store_type, store_path)
}
