use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Logging configuration loaded from logging.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    pub console: ConsoleConfig,
    pub file: FileConfig,
    #[serde(default)]
    pub modules: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleConfig {
    pub enable: bool,
    pub level: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileConfig {
    pub enable: bool,
    pub level: String,
    pub path: String,
    pub max_size_mb: u32,
    pub max_files: u32,
    pub rotation: String,
}

impl LogConfig {
    /// Load logging configuration from logging.json in the config directory.
    /// If the file doesn't exist, returns default configuration.
    pub fn load(config_dir: &str) -> Result<Self, String> {
        let path = format!("{}/logging.json", config_dir);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                serde_json::from_str(&content)
                    .map_err(|e| format!("Parse {} error: {}", path, e))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::default())
            }
            Err(e) => Err(format!("Failed to read {}: {}", path, e)),
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            description: None,
            console: ConsoleConfig {
                enable: true,
                level: "info".to_string(),
                format: "compact".to_string(),
            },
            file: FileConfig {
                enable: false,
                level: "debug".to_string(),
                path: "logs/flowpulse.log".to_string(),
                max_size_mb: 50,
                max_files: 5,
                rotation: "daily".to_string(),
            },
            modules: HashMap::new(),
        }
    }
}