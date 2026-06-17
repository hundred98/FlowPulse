//! Safety module configuration
//!
//! This module consolidates all configuration types used by the safety subsystem,
//! including motion limits, temperature safety thresholds, and general settings.

use std::collections::HashMap;

/// Motion limit configuration for a single axis
#[derive(Debug, Clone, Copy)]
pub struct MotionLimit {
    /// Minimum position (mm)
    pub min: f32,

    /// Maximum position (mm)
    pub max: f32,
}

/// Temperature limit configuration (absolute bounds)
#[derive(Debug, Clone, Copy)]
pub struct TemperatureLimit {
    /// Minimum temperature (°C)
    pub min: f32,

    /// Maximum temperature (°C)
    pub max: f32,
}

/// Sensor fault detection thresholds
#[derive(Debug, Clone)]
pub struct SensorFaultConfig {
    /// Maximum plausible temperature (°C) — above this = sensor fault
    pub max_temp: f32,

    /// Minimum plausible temperature (°C) — below this = sensor fault
    pub min_temp: f32,
}

impl Default for SensorFaultConfig {
    fn default() -> Self {
        Self {
            max_temp: 300.0,
            min_temp: -50.0,
        }
    }
}

/// Deviation thresholds for temperature safety
#[derive(Debug, Clone)]
pub struct DeviationThresholds {
    /// Warning level deviation (°C)
    pub warning: f32,
    /// Critical level deviation (°C)
    pub critical: f32,
    /// Emergency level deviation (°C)
    pub emergency: f32,
}

impl Default for DeviationThresholds {
    fn default() -> Self {
        Self {
            warning: 10.0,
            critical: 15.0,
            emergency: 20.0,
        }
    }
}

/// Action configuration for a specific level
#[derive(Debug, Clone)]
pub struct LevelActionConfig {
    /// Action string: "warn", "pause_print", "turn_off", "emergency_stop"
    pub warning: String,
    pub critical: String,
    pub emergency: String,
}

impl Default for LevelActionConfig {
    fn default() -> Self {
        Self {
            warning: "warn".to_string(),
            critical: "pause_print".to_string(),
            emergency: "emergency_stop".to_string(),
        }
    }
}

/// Per-heater safety configuration
#[derive(Debug, Clone)]
pub struct HeaterSafetyConfig {
    /// Deviation thresholds (how far from target before action)
    pub deviation_thresholds: DeviationThresholds,

    /// Actions for low temperature
    pub low_temp: LevelActionConfig,

    /// Actions for high temperature
    pub high_temp: LevelActionConfig,

    /// Heating delay (seconds) before low-temp warnings activate
    pub heating_delay_secs: u32,

    /// Sensor fault detection thresholds
    pub sensor_fault: SensorFaultConfig,
}

impl Default for HeaterSafetyConfig {
    fn default() -> Self {
        Self {
            deviation_thresholds: DeviationThresholds::default(),
            low_temp: LevelActionConfig {
                warning: "warn".to_string(),
                critical: "warn".to_string(),
                emergency: "pause_print".to_string(),
            },
            high_temp: LevelActionConfig {
                warning: "warn".to_string(),
                critical: "turn_off".to_string(),
                emergency: "emergency_stop".to_string(),
            },
            heating_delay_secs: 60,
            sensor_fault: SensorFaultConfig::default(),
        }
    }
}

/// Temperature safety configuration
#[derive(Debug, Clone)]
pub struct TemperatureSafetyConfig {
    /// Per-heater configurations (keyed by heater name: "bed", "hotend", etc.)
    pub heaters: HashMap<String, HeaterSafetyConfig>,
}

impl Default for TemperatureSafetyConfig {
    fn default() -> Self {
        let mut heaters = HashMap::new();
        heaters.insert("hotend".to_string(), HeaterSafetyConfig::default());
        heaters.insert("bed".to_string(), HeaterSafetyConfig {
            deviation_thresholds: DeviationThresholds {
                warning: 5.0,
                critical: 10.0,
                emergency: 15.0,
            },
            low_temp: LevelActionConfig {
                warning: "warn".to_string(),
                critical: "warn".to_string(),
                emergency: "pause_print".to_string(),
            },
            high_temp: LevelActionConfig {
                warning: "warn".to_string(),
                critical: "turn_off".to_string(),
                emergency: "turn_off".to_string(),
            },
            heating_delay_secs: 120,
            sensor_fault: SensorFaultConfig::default(),
        });
        Self { heaters }
    }
}

/// Unified safety module configuration
#[derive(Debug, Clone)]
pub struct SafetyConfig {
    /// Enable all safety checks
    pub enable_safety_checks: bool,

    /// Safety check interval (milliseconds)
    pub check_interval_ms: u64,

    /// Emergency stop timeout (milliseconds)
    pub emergency_stop_timeout_ms: u64,

    /// Motion limits for axes (keyed by axis name: "x", "y", "z")
    pub motion_limits: HashMap<String, MotionLimit>,

    /// Temperature safety configuration
    pub temperature: TemperatureSafetyConfig,

    /// Device state staleness threshold (milliseconds)
    pub state_stale_threshold_ms: u64,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        let mut motion_limits = HashMap::new();
        motion_limits.insert("x".to_string(), MotionLimit { min: 0.0, max: 300.0 });
        motion_limits.insert("y".to_string(), MotionLimit { min: 0.0, max: 300.0 });
        motion_limits.insert("z".to_string(), MotionLimit { min: 0.0, max: 400.0 });

        Self {
            enable_safety_checks: true,
            check_interval_ms: 1000,
            emergency_stop_timeout_ms: 1000,
            motion_limits,
            temperature: TemperatureSafetyConfig::default(),
            state_stale_threshold_ms: 5000,
        }
    }
}