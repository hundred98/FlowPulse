//! Temperature management module
//!
//! This module provides comprehensive temperature management for the printer system,
//! including temperature state management, safety checks, temperature presets,
//! and PID auto-tuning.

pub mod types;
pub mod safety;
pub mod preset;
pub mod manager;
pub mod pid_tune;

pub use types::{
    HeaterState, TemperaturePreset, TemperatureManagerConfig,
    SafetyLevel, SafetyAction, SafetyCheckResult,
};
pub use safety::TemperatureSafetyChecker;
pub use preset::PresetManager;
pub use manager::TemperatureManager;
pub use pid_tune::{
    PidTuneProtocol, PidTuneSubType, PidTuneResult, PidParams,
    TunePhase, TuneProgress, TuneErrorCode,
};
