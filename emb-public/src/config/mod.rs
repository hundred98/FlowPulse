//! Configuration module
//!
//! This module contains all configuration-related functionality:
//! - `config_manager`: Centralized configuration management (single entry point)
//! - `printer_config`: Printer configuration structures and JSON parsing
//! - `config_adapter`: Configuration adapter for merging multiple config files
//! - `config_protocol`: Configuration frame builder for STM32 communication
//!
//! # Usage
//! All configuration access must go through `ConfigManager`:
//! ```ignore
//! use emb_public::config::ConfigManager;
//!
//! // Load configuration at startup
//! ConfigManager::instance().load("./config")?;
//!
//! // Get configuration
//! let config = ConfigManager::instance().get_config()?;
//!
//! // Reload configuration (user triggered)
//! ConfigManager::instance().reload(&client).await?;
//! ```

pub mod config_manager;
pub mod printer_config;
pub mod config_adapter;
pub mod config_protocol;

// Re-export ConfigManager as the primary interface
pub use config_manager::ConfigManager;

// Re-export configuration types (but not loading functions)
pub use printer_config::{
    PrinterJsonConfig, PrinterParams, MotorParams, LimitSwitchParams,
    TemperatureParams, HeaterParams, FanParams, ProbeParams,
    LimitSwitchAxis, TempSensorParams, HeaterPin, TemperaturePresetConfig,
    TemperatureSafetyConfig, TempHeaterSafetyConfig, SensorFaultConfig,
    DeviationThresholdsConfig, HeaterActionsConfig, TemperatureActionsConfig,
    PidTuneParams, PidTuneHeaterConfig,
};

// Re-export protocol types for frame building
pub use config_protocol::{
    ConfigFrameBuilder, create_config_frames, validate_config,
};

// Note: load_config_from_file, load_configs, and configure_device are NOT re-exported.
// Use ConfigManager::load() and ConfigManager::reload() instead.
