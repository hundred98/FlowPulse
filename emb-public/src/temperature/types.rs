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

    /// Sensor fault detection thresholds
    pub sensor_fault_max_temp: f32,
    pub sensor_fault_min_temp: f32,

    /// Heating start time (when target was set)
    pub heating_start: Option<Instant>,

    /// Sensor fault detected
    pub sensor_fault: bool,
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
            sensor_fault_max_temp: 300.0,
            sensor_fault_min_temp: -50.0,
            heating_start: None,
            sensor_fault: false,
        }
    }

    /// Create a new heater state with sensor fault thresholds
    pub fn with_sensor_fault_thresholds(
        name: String,
        heater_id: u8,
        min_temp: f32,
        max_temp: f32,
        sensor_fault_max_temp: f32,
        sensor_fault_min_temp: f32,
    ) -> Self {
        Self {
            name,
            heater_id,
            current_temp: 0.0,
            target_temp: 0.0,
            is_heating: false,
            last_update: Instant::now(),
            min_temp,
            max_temp,
            sensor_fault_max_temp,
            sensor_fault_min_temp,
            heating_start: None,
            sensor_fault: false,
        }
    }

    /// Update current temperature
    pub fn update_current(&mut self, temp: f32) {
        self.current_temp = temp;
        self.last_update = Instant::now();

        // Check for sensor fault (abnormal values)
        self.sensor_fault = temp > self.sensor_fault_max_temp || temp < self.sensor_fault_min_temp;
    }

    /// Set target temperature
    pub fn set_target(&mut self, temp: f32) {
        let was_heating = self.is_heating;
        self.target_temp = temp;
        self.is_heating = temp > 0.0;

        // Track heating start time
        if self.is_heating && !was_heating {
            self.heating_start = Some(Instant::now());
        } else if !self.is_heating {
            self.heating_start = None;
        }
    }

    /// Check if temperature is within safe range
    pub fn is_safe(&self) -> bool {
        !self.sensor_fault && self.current_temp >= self.min_temp && self.current_temp <= self.max_temp
    }

    /// Get temperature deviation from target
    pub fn deviation(&self) -> f32 {
        self.current_temp - self.target_temp
    }

    /// Get heating duration (seconds)
    pub fn heating_duration_secs(&self) -> f64 {
        self.heating_start
            .map(|start| start.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    }

    /// Check if sensor is faulty
    pub fn has_sensor_fault(&self) -> bool {
        self.sensor_fault
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

/// Temperature wait configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureWaitConfig {
    /// Maximum time to wait for temperature to reach target (seconds)
    pub timeout_secs: u64,

    /// Temperature tolerance (°C) — considered "reached" when |current - target| <= tolerance
    pub tolerance: f32,

    /// Interval between temperature checks (milliseconds)
    pub check_interval_ms: u64,

    /// Number of consecutive stable checks required to confirm temperature reached
    pub stable_count: u32,
}

impl Default for TemperatureWaitConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 300,
            tolerance: 2.0,
            check_interval_ms: 500,
            stable_count: 3,
        }
    }
}

/// Auto-fan configuration
///
/// Automatically turns on/off a GPIO fan based on temperature thresholds.
/// This protects the hotend from heat creep (fan off at low temp) and
/// ensures cooling when hot (fan on above threshold).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoFanConfig {
    /// Enable auto-fan
    pub enable: bool,

    /// GPIO pin name for the hotend fan (e.g. "hotend_fan")
    pub gpio_name: String,

    /// Temperature (°C) above which the fan turns on
    pub on_threshold: f32,

    /// Temperature (°C) below which the fan turns off
    /// Use hysteresis (on > off) to prevent rapid toggling
    pub off_threshold: f32,

    /// Fan value when on (0.0–1.0, default 1.0 = 100%)
    pub on_value: f32,
}

impl Default for AutoFanConfig {
    fn default() -> Self {
        Self {
            enable: true,
            gpio_name: "hotend_fan".to_string(),
            on_threshold: 60.0,
            off_threshold: 50.0,
            on_value: 1.0,
        }
    }
}

/// Temperature manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureManagerConfig {
    /// Safety check interval (milliseconds)
    pub safety_check_interval_ms: u64,

    /// Temperature change threshold for triggering events
    pub temp_change_threshold: f32,

    /// Enable automatic safety checks
    pub enable_auto_safety_check: bool,

    /// Temperature wait configuration (M109/M190)
    pub wait: TemperatureWaitConfig,

    /// Auto-fan configuration
    pub auto_fan: AutoFanConfig,
}

impl Default for TemperatureManagerConfig {
    fn default() -> Self {
        Self {
            safety_check_interval_ms: 1000,
            temp_change_threshold: 1.0,
            enable_auto_safety_check: true,
            wait: TemperatureWaitConfig::default(),
            auto_fan: AutoFanConfig::default(),
        }
    }
}

/// Safety check level — re-exported from unified safety module
pub use crate::safety::types::{SafetyLevel, SafetyAction, SafetyCheckResult};

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
        let result = SafetyCheckResult::warning("temperature", "deviation", "Temperature too low")
            .with_temps(180.0, 200.0);

        assert_eq!(result.source, "temperature");
        assert_eq!(result.level, SafetyLevel::Warning);
        assert_eq!(result.action, SafetyAction::LogWarning);
        assert_eq!(result.current_temp, Some(180.0));
        assert_eq!(result.target_temp, Some(200.0));
        assert!(result.needs_action());
    }
}
