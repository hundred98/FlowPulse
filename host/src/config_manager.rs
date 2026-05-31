//! Configuration File Manager
//!
//! This module provides configuration file management including:
//! - JSON/YAML configuration file loading and saving
//! - Configuration validation
//! - Hot-reload support
//! - Default configuration generation

use std::path::{Path, PathBuf};
use std::fs;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use serde::{Deserialize, Serialize};
use serde_json;

use emb_public::EmbError;
use crate::realtime_monitor::MonitoringConfig;

/// Configuration file format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigFormat {
    Json,
    Yaml,
}

impl ConfigFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "json" => Some(ConfigFormat::Json),
            "yaml" | "yml" => Some(ConfigFormat::Yaml),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ConfigFormat::Json => "json",
            ConfigFormat::Yaml => "yaml",
        }
    }
}

/// Complete printer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterConfig {
    pub printer_id: String,
    pub printer_name: String,
    pub printer_model: String,
    pub monitoring: MonitoringConfig,
    pub gcode_settings: GCodeSettings,
    pub paths: PathSettings,
    pub network: NetworkSettings,
    pub ui: UISettings,
    pub advanced: AdvancedSettings,
}

/// G-code processing settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCodeSettings {
    pub enable_preprocessing: bool,
    pub remove_comments: bool,
    pub optimize_moves: bool,
    pub min_movement_distance: f32,
    pub max_file_size_mb: u32,
    pub default_print_speed: u32,
    pub default_travel_speed: u32,
    pub motion_batch_size: u8,
}

/// Path settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathSettings {
    pub gcode_directory: String,
    pub config_directory: String,
    pub log_directory: String,
    pub temp_directory: String,
    pub backup_directory: String,
    pub firmware_directory: String,
}

/// Network settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSettings {
    pub wifi_enabled: bool,
    pub wifi_ssid: Option<String>,
    pub wifi_password: Option<String>,
    pub use_static_ip: bool,
    pub static_ip: Option<String>,
    pub subnet_mask: Option<String>,
    pub gateway: Option<String>,
    pub dns_server: Option<String>,
    pub web_port: u16,
    pub enable_https: bool,
    pub api_token: Option<String>,
}

/// UI settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UISettings {
    pub language: String,
    pub temperature_unit: String,
    pub length_unit: String,
    pub dark_mode: bool,
    pub enable_sounds: bool,
    pub notifications: NotificationSettings,
}

/// Notification settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub print_complete: bool,
    pub print_error: bool,
    pub temp_alerts: bool,
    pub progress_updates: bool,
    pub progress_interval_min: u32,
}

/// Advanced settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedSettings {
    pub log_level: String,
    pub debug_mode: bool,
    pub auto_save_interval_sec: u32,
    pub config_backup_count: u32,
    pub enable_hot_reload: bool,
    pub hot_reload_interval_sec: u32,
    pub max_log_size_mb: u32,
    pub max_log_files: u32,
}

impl Default for PrinterConfig {
    fn default() -> Self {
        Self {
            printer_id: uuid::Uuid::new_v4().to_string(),
            printer_name: "3D Printer".to_string(),
            printer_model: "Generic".to_string(),
            monitoring: MonitoringConfig::default(),
            gcode_settings: GCodeSettings::default(),
            paths: PathSettings::default(),
            network: NetworkSettings::default(),
            ui: UISettings::default(),
            advanced: AdvancedSettings::default(),
        }
    }
}

impl Default for GCodeSettings {
    fn default() -> Self {
        Self {
            enable_preprocessing: true,
            remove_comments: true,
            optimize_moves: true,
            min_movement_distance: 0.01,
            max_file_size_mb: 100,
            default_print_speed: 0,
            default_travel_speed: 0,
            motion_batch_size: 4,
        }
    }
}

impl Default for PathSettings {
    fn default() -> Self {
        Self {
            gcode_directory: "./gcode".to_string(),
            config_directory: "./config".to_string(),
            log_directory: "./logs".to_string(),
            temp_directory: "./tmp".to_string(),
            backup_directory: "./backups".to_string(),
            firmware_directory: "./firmware".to_string(),
        }
    }
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            wifi_enabled: false,
            wifi_ssid: None,
            wifi_password: None,
            use_static_ip: false,
            static_ip: None,
            subnet_mask: None,
            gateway: None,
            dns_server: None,
            web_port: 8080,
            enable_https: false,
            api_token: None,
        }
    }
}

impl Default for UISettings {
    fn default() -> Self {
        Self {
            language: "zh".to_string(),
            temperature_unit: "celsius".to_string(),
            length_unit: "mm".to_string(),
            dark_mode: true,
            enable_sounds: true,
            notifications: NotificationSettings::default(),
        }
    }
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            print_complete: true,
            print_error: true,
            temp_alerts: true,
            progress_updates: false,
            progress_interval_min: 10,
        }
    }
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            debug_mode: false,
            auto_save_interval_sec: 300,
            config_backup_count: 5,
            enable_hot_reload: true,
            hot_reload_interval_sec: 5,
            max_log_size_mb: 10,
            max_log_files: 10,
        }
    }
}

/// Configuration change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigChangeEvent {
    Loaded { path: String },
    Saved { path: String },
    Reloaded { path: String },
    Changed { section: String },
    Error { message: String },
}

/// Configuration manager
pub struct ConfigManager {
    config: Arc<RwLock<PrinterConfig>>,
    pub config_path: PathBuf,
    format: ConfigFormat,
    last_modified: Arc<RwLock<SystemTime>>,
    change_tx: broadcast::Sender<ConfigChangeEvent>,
    hot_reload_enabled: Arc<RwLock<bool>>,
    auto_save_enabled: Arc<RwLock<bool>>,
}

impl ConfigManager {
    pub fn new() -> Self {
        let (change_tx, _) = broadcast::channel(100);
        
        Self {
            config: Arc::new(RwLock::new(PrinterConfig::default())),
            config_path: PathBuf::from("./config/printer.json"),
            format: ConfigFormat::Json,
            last_modified: Arc::new(RwLock::new(SystemTime::UNIX_EPOCH)),
            change_tx,
            hot_reload_enabled: Arc::new(RwLock::new(false)),
            auto_save_enabled: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, EmbError> {
        let path = path.as_ref().to_path_buf();
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| EmbError::StateMachine("Invalid config file extension".to_string()))?;
        
        let format = ConfigFormat::from_extension(ext)
            .ok_or_else(|| EmbError::StateMachine(format!("Unsupported config format: {}", ext)))?;

        let mut manager = Self {
            config: Arc::new(RwLock::new(PrinterConfig::default())),
            config_path: path,
            format,
            last_modified: Arc::new(RwLock::new(SystemTime::UNIX_EPOCH)),
            change_tx: broadcast::channel(100).0,
            hot_reload_enabled: Arc::new(RwLock::new(false)),
            auto_save_enabled: Arc::new(RwLock::new(false)),
        };

        manager.load().await?;
        Ok(manager)
    }

    pub async fn load(&mut self) -> Result<(), EmbError> {
        let path = &self.config_path;
        
        if !path.exists() {
            self.save().await?;
            return Ok(());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| EmbError::StateMachine(format!("Failed to read config file: {}", e)))?;

        let config: PrinterConfig = match self.format {
            ConfigFormat::Json => {
                serde_json::from_str(&content)
                    .map_err(|e| EmbError::StateMachine(format!("Failed to parse JSON config: {}", e)))?
            }
            ConfigFormat::Yaml => {
                return Err(EmbError::StateMachine("YAML support not implemented".to_string()));
            }
        };

        *self.config.write().await = config;
        
        let metadata = fs::metadata(path)
            .map_err(|e| EmbError::StateMachine(format!("Failed to read file metadata: {}", e)))?;
        let modified = metadata.modified()
            .map_err(|e| EmbError::StateMachine(format!("Failed to get modification time: {}", e)))?;
        *self.last_modified.write().await = modified;

        let _ = self.change_tx.send(ConfigChangeEvent::Loaded {
            path: path.to_string_lossy().to_string(),
        });

        Ok(())
    }

    pub async fn save(&self) -> Result<(), EmbError> {
        let path = &self.config_path;
        
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| EmbError::StateMachine(format!("Failed to create config directory: {}", e)))?;
        }

        let config = self.config.read().await;
        
        let content = match self.format {
            ConfigFormat::Json => {
                serde_json::to_string_pretty(&*config)
                    .map_err(|e| EmbError::StateMachine(format!("Failed to serialize config: {}", e)))?
            }
            ConfigFormat::Yaml => {
                return Err(EmbError::StateMachine("YAML support not implemented".to_string()));
            }
        };

        fs::write(path, content)
            .map_err(|e| EmbError::StateMachine(format!("Failed to write config file: {}", e)))?;

        let _ = self.change_tx.send(ConfigChangeEvent::Saved {
            path: path.to_string_lossy().to_string(),
        });

        Ok(())
    }

    pub async fn get_config(&self) -> PrinterConfig {
        self.config.read().await.clone()
    }

    pub async fn update_config<F>(&self, f: F) -> Result<(), EmbError>
    where
        F: FnOnce(&mut PrinterConfig),
    {
        let mut config = self.config.write().await;
        f(&mut config);
        drop(config);
        
        if *self.auto_save_enabled.read().await {
            self.save().await?;
        }

        let _ = self.change_tx.send(ConfigChangeEvent::Changed {
            section: "general".to_string(),
        });

        Ok(())
    }

    pub fn subscribe_changes(&self) -> broadcast::Receiver<ConfigChangeEvent> {
        self.change_tx.subscribe()
    }
}