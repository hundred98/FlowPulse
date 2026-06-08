//! Temperature types and data structures
//!
//! This module defines all types used by the temperature management system.

use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Heater state
#[derive(Debug, Clone)]
pub struct HeaterState {
    /// Heater name (hotend, bed, chamber, etc.)
    pub name: String,

    /// Heater ID (used in config frames)
    pub heater_id: u8,

    /// Current temperature (°C)
    pub current_temp: f32,

    /// Target temperature (°C)
    pub target_temp: f32,

    /// Whether the heater is currently heating
    pub is_heating: bool,

    /// Last update time
    pub last_update: Instant,

    /// Temperature limits (read from config)
    pub min_temp: f32,
    pub max_temp: f32,
}

impl HeaterState {
    /// Create a new heater state
    pub fn new(name: String, heater_id: u8, min_temp: f32, max_temp: f32) -> Self {
        Self {
            name,
            heater_id,
            current_temp: 0.0,
            target_temp: 0.0,
            is_heating: false,
            last_update: Instant::now(),
            min_temp,
            max_temp,
        }
    }

    /// Update current temperature
    pub fn update_current(&mut self, temp: f32) {
        self.current_temp = temp;
        self.last_update = Instant::now();
    }

    /// Set target temperature
    pub fn set_target(&mut self, temp: f32) {
        self.target_temp = temp;
        self.is_heating = temp > 0.0;
    }

    /// Check if temperature is within safe range
    pub fn is_safe(&self) -> bool {
        self.current_temp >= self.min_temp && self.current_temp <= self.max_temp
    }

    /// Get temperature deviation from target
    pub fn deviation(&self) -> f32 {
        self.current_temp - self.target_temp
    }
}

/// Temperature preset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperaturePreset {
    /// Preset name (PLA, ABS, PETG, etc.)
    pub name: String,

    /// Hotend temperature
    pub hotend_temp: f32,

    /// Bed temperature
    pub bed_temp: f32,

    /// Chamber temperature (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chamber_temp: Option<f32>,

    /// Fan speed (0-100%)
    #[serde(default)]
    pub fan_speed: u8,
}

impl TemperaturePreset {
    /// Create a new temperature preset
    pub fn new(name: String, hotend_temp: f32, bed_temp: f32) -> Self {
        Self {
            name,
            hotend_temp,
            bed_temp,
            chamber_temp: None,
            fan_speed: 100,
        }
    }

    /// Create a preset with chamber temperature
    pub fn with_chamber(mut self, chamber_temp: f32) -> Self {
        self.chamber_temp = Some(chamber_temp);
        self
    }

    /// Create a preset with fan speed
    pub fn with_fan(mut self, fan_speed: u8) -> Self {
        self.fan_speed = fan_speed;
        self
    }
}

impl Default for TemperaturePreset {
    fn default() -> Self {
        Self {
            name: "PLA".to_string(),
            hotend_temp: 200.0,
            bed_temp: 60.0,
            chamber_temp: None,
            fan_speed: 100,
        }
    }
}

/// Temperature manager configuration
#[derive(Debug, Clone)]
pub struct TemperatureManagerConfig {
    /// Safety check interval (milliseconds)
    pub safety_check_interval_ms: u64,

    /// Temperature change threshold for triggering events
    pub temp_change_threshold: f32,

    /// Enable automatic safety checks
    pub enable_auto_safety_check: bool,

    /// Maximum temperature deviation before warning (°C)
    pub max_deviation_warning: f32,

    /// Maximum temperature deviation before critical (°C)
    pub max_deviation_critical: f32,

    /// Maximum temperature deviation before emergency stop (°C)
    pub max_deviation_emergency: f32,
}

impl Default for TemperatureManagerConfig {
    fn default() -> Self {
        Self {
            safety_check_interval_ms: 1000,
            temp_change_threshold: 1.0,
            enable_auto_safety_check: true,
            max_deviation_warning: 10.0,
            max_deviation_critical: 15.0,
            max_deviation_emergency: 20.0,
        }
    }
}

/// Safety check level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyLevel {
    /// Normal operation
    Normal,

    /// Warning (minor temperature deviation)
    Warning,

    /// Critical (requires pausing print)
    Critical,

    /// Dangerous (requires emergency stop)
    Dangerous,
}

impl Default for SafetyLevel {
    fn default() -> Self {
        Self::Normal
    }
}

/// Safety action to take
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyAction {
    /// No action needed
    None,

    /// Publish warning event
    Warn,

    /// Turn off heater
    TurnOffHeater,

    /// Pause print
    PausePrint,

    /// Emergency stop
    EmergencyStop,
}

impl Default for SafetyAction {
    fn default() -> Self {
        Self::None
    }
}

/// Safety check result
#[derive(Debug, Clone)]
pub struct SafetyCheckResult {
    /// Heater name
    pub heater: String,

    /// Check level
    pub level: SafetyLevel,

    /// Problem description
    pub message: String,

    /// Suggested action
    pub action: SafetyAction,

    /// Current temperature
    pub current_temp: f32,

    /// Target temperature
    pub target_temp: f32,

    /// Temperature deviation
    pub deviation: f32,
}

impl SafetyCheckResult {
    /// Create a new safety check result
    pub fn new(
        heater: String,
        level: SafetyLevel,
        message: String,
        action: SafetyAction,
    ) -> Self {
        Self {
            heater,
            level,
            message,
            action,
            current_temp: 0.0,
            target_temp: 0.0,
            deviation: 0.0,
        }
    }

    /// Create a normal result
    pub fn normal(heater: String) -> Self {
        Self::new(heater, SafetyLevel::Normal, "Temperature normal".to_string(), SafetyAction::None)
    }

    /// Create a warning result
    pub fn warning(heater: String, message: String) -> Self {
        Self::new(heater, SafetyLevel::Warning, message, SafetyAction::Warn)
    }

    /// Create a critical result
    pub fn critical(heater: String, message: String) -> Self {
        Self::new(heater, SafetyLevel::Critical, message, SafetyAction::PausePrint)
    }

    /// Create a dangerous result
    pub fn dangerous(heater: String, message: String) -> Self {
        Self::new(heater, SafetyLevel::Dangerous, message, SafetyAction::EmergencyStop)
    }

    /// Set temperature data
    pub fn with_temps(mut self, current: f32, target: f32) -> Self {
        self.current_temp = current;
        self.target_temp = target;
        self.deviation = current - target;
        self
    }

    /// Check if action is needed
    pub fn needs_action(&self) -> bool {
        self.action != SafetyAction::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heater_state() {
        let mut heater = HeaterState::new("hotend".to_string(), 1, 0.0, 250.0);

        assert_eq!(heater.current_temp, 0.0);
        assert_eq!(heater.target_temp, 0.0);
        assert!(!heater.is_heating);

        heater.update_current(25.0);
        assert_eq!(heater.current_temp, 25.0);

        heater.set_target(200.0);
        assert_eq!(heater.target_temp, 200.0);
        assert!(heater.is_heating);

        assert_eq!(heater.deviation(), 25.0 - 200.0);
    }

    #[test]
    fn test_temperature_preset() {
        let preset = TemperaturePreset::new("PLA".to_string(), 200.0, 60.0)
            .with_fan(100);

        assert_eq!(preset.name, "PLA");
        assert_eq!(preset.hotend_temp, 200.0);
        assert_eq!(preset.bed_temp, 60.0);
        assert_eq!(preset.fan_speed, 100);
        assert_eq!(preset.chamber_temp, None);
    }

    #[test]
    fn test_safety_check_result() {
        let result = SafetyCheckResult::warning("hotend".to_string(), "Temperature too low".to_string())
            .with_temps(180.0, 200.0);

        assert_eq!(result.heater, "hotend");
        assert_eq!(result.level, SafetyLevel::Warning);
        assert_eq!(result.action, SafetyAction::Warn);
        assert_eq!(result.deviation, -20.0);
        assert!(result.needs_action());
    }
}
