//! Configuration module
//!
//! This module contains all configuration-related functionality:
//! - `printer_config`: Printer configuration structures and JSON parsing
//! - `config_adapter`: Configuration adapter for merging multiple config files
//! - `config_protocol`: Configuration frame builder for STM32 communication

pub mod printer_config;
pub mod config_adapter;
pub mod config_protocol;

// Re-export commonly used types
pub use printer_config::{
    PrinterJsonConfig, PrinterParams, MotorParams, LimitSwitchParams,
    TemperatureParams, HeaterParams, FanParams, ProbeParams,
    load_config_from_file, parse_json_config,
};

pub use config_adapter::{
    load_configs, build_motion_config_json, build_printer_config,
    LoadedConfigs, HardwareConfig, MotionFileConfig, PrinterFileConfig,
};

pub use config_protocol::{
    ConfigFrameBuilder, create_config_frames, validate_config,
};
