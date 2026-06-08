//! Temperature management module
//!
//! This module provides comprehensive temperature management for the printer system,
//! including temperature state management, safety checks, and temperature presets.

pub mod types;
pub mod safety;
pub mod preset;
pub mod manager;

pub use types::{
    HeaterState, TemperaturePreset, TemperatureManagerConfig,
    SafetyLevel, SafetyAction, SafetyCheckResult,
};
pub use safety::TemperatureSafetyChecker;
pub use preset::PresetManager;
pub use manager::TemperatureManager;
