//! Temperature safety checker
//!
//! This module provides safety checking for temperature management,
//! including temperature deviation detection and safety action determination.
//! This is a port of the original `temperature::safety::TemperatureSafetyChecker`
//! adapted to use the unified safety types from `safety::types`.

use super::config::{HeaterSafetyConfig, SafetyConfig, TemperatureSafetyConfig};
use super::types::{SafetyAction, SafetyCheckResult, SafetyLevel};
use std::collections::HashMap;

/// Reference to a heater state for safety checking.
///
/// This is the minimum data the checker needs, avoiding a direct dependency
/// on `temperature::types::HeaterState`.
#[derive(Debug, Clone)]
pub struct HeaterReading {
    /// Heater name ("bed", "hotend", etc.)
    pub name: String,

    /// Current temperature (°C)
    pub current_temp: f32,

    /// Target temperature (°C)
    pub target_temp: f32,

    /// Whether the heater is actively heating (target > 0)
    pub is_heating: bool,

    /// How long the heater has been heating (seconds)
    pub heating_duration_secs: f64,

    /// Absolute minimum safe temperature (°C)
    pub min_temp: f32,

    /// Absolute maximum safe temperature (°C)
    pub max_temp: f32,

    /// Sensor fault detected (abnormal temperature reading)
    pub sensor_fault: bool,
}

/// Temperature safety checker
pub struct TemperatureSafetyChecker {
    /// Per-heater safety configuration
    heater_configs: HashMap<String, HeaterSafetyConfig>,

    /// Sensor fault detection thresholds (fallback)
    sensor_fault_max_temp: f32,
    sensor_fault_min_temp: f32,
}

impl TemperatureSafetyChecker {
    /// Create a new safety checker from config
    pub fn new(config: &SafetyConfig) -> Self {
        let heater_configs = config.temperature.heaters.clone();
        Self {
            heater_configs,
            sensor_fault_max_temp: 300.0,
            sensor_fault_min_temp: -50.0,
        }
    }

    /// Create from a temperature-specific safety config
    pub fn from_temp_config(config: &TemperatureSafetyConfig) -> Self {
        Self {
            heater_configs: config.heaters.clone(),
            sensor_fault_max_temp: 300.0,
            sensor_fault_min_temp: -50.0,
        }
    }

    /// Reload configuration
    pub fn reload(&mut self, config: &SafetyConfig) {
        self.heater_configs = config.temperature.heaters.clone();
    }

    /// Check a single heater for safety issues
    pub fn check_heater(&self, state: &HeaterReading) -> SafetyCheckResult {
        // 0. Check for sensor fault first
        if state.sensor_fault || self.detect_sensor_fault(state) {
            return SafetyCheckResult::emergency(
                "temperature",
                format!("sensor_fault/{}", state.name),
                format!(
                    "Sensor fault on '{}': {:.1}°C (abnormal value)",
                    state.name, state.current_temp
                ),
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        // 1. Check absolute temperature bounds
        if state.current_temp < state.min_temp {
            return SafetyCheckResult::emergency(
                "temperature",
                format!("min_temp/{}", state.name),
                format!(
                    "Temperature below minimum on '{}': {:.1}°C < {:.1}°C",
                    state.name, state.current_temp, state.min_temp
                ),
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        if state.current_temp > state.max_temp {
            return SafetyCheckResult::emergency(
                "temperature",
                format!("max_temp/{}", state.name),
                format!(
                    "Temperature above maximum on '{}': {:.1}°C > {:.1}°C",
                    state.name, state.current_temp, state.max_temp
                ),
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        let deviation = state.current_temp - state.target_temp;
        let is_heating_up = state.is_heating && deviation < 0.0;

        // Get heater-specific configuration
        let heater_config = self.heater_configs.get(&state.name);

        // Get deviation thresholds
        let (warning_threshold, critical_threshold, emergency_threshold) = heater_config
            .map(|c| {
                (
                    c.deviation_thresholds.warning,
                    c.deviation_thresholds.critical,
                    c.deviation_thresholds.emergency,
                )
            })
            .unwrap_or((10.0, 15.0, 20.0));

        // 2. Check low temperature deviation
        if !is_heating_up {
            let abs_deviation = -deviation; // positive value for "how far below target"
            if abs_deviation > emergency_threshold {
                let (level, action) = self.get_low_temp_action(&state.name, state.heating_duration_secs, heater_config);
                return SafetyCheckResult::new(
                    "temperature",
                    format!("deviation_low/{}", state.name),
                    level,
                    action,
                    format!(
                        "Temperature too low on '{}': {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                        state.name, state.current_temp, state.target_temp, deviation
                    ),
                )
                .with_temps(state.current_temp, state.target_temp);
            }

            if abs_deviation > critical_threshold {
                let (level, action) = self.get_low_temp_action(&state.name, state.heating_duration_secs, heater_config);
                return SafetyCheckResult::new(
                    "temperature",
                    format!("deviation_low/{}", state.name),
                    level,
                    action,
                    format!(
                        "Temperature low on '{}': {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                        state.name, state.current_temp, state.target_temp, deviation
                    ),
                )
                .with_temps(state.current_temp, state.target_temp);
            }

            if abs_deviation > warning_threshold {
                let (level, action) = self.get_low_temp_action(&state.name, state.heating_duration_secs, heater_config);
                return SafetyCheckResult::new(
                    "temperature",
                    format!("deviation_low/{}", state.name),
                    level,
                    action,
                    format!(
                        "Temperature slightly low on '{}': {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                        state.name, state.current_temp, state.target_temp, deviation
                    ),
                )
                .with_temps(state.current_temp, state.target_temp);
            }
        }

        // 3. Check high temperature deviation (only when actively heating)
        if state.is_heating {
            if deviation > emergency_threshold {
                let (level, action) = self.get_high_temp_action(&state.name, heater_config);
                return SafetyCheckResult::new(
                    "temperature",
                    format!("deviation_high/{}", state.name),
                    level,
                    action,
                    format!(
                        "Temperature too high on '{}': {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                        state.name, state.current_temp, state.target_temp, deviation
                    ),
                )
                .with_temps(state.current_temp, state.target_temp);
            }

            if deviation > critical_threshold {
                let (level, action) = self.get_high_temp_action(&state.name, heater_config);
                return SafetyCheckResult::new(
                    "temperature",
                    format!("deviation_high/{}", state.name),
                    level,
                    action,
                    format!(
                        "Temperature high on '{}': {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                        state.name, state.current_temp, state.target_temp, deviation
                    ),
                )
                .with_temps(state.current_temp, state.target_temp);
            }

            if deviation > warning_threshold {
                let (level, action) = self.get_high_temp_action(&state.name, heater_config);
                return SafetyCheckResult::new(
                    "temperature",
                    format!("deviation_high/{}", state.name),
                    level,
                    action,
                    format!(
                        "Temperature slightly high on '{}': {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                        state.name, state.current_temp, state.target_temp, deviation
                    ),
                )
                .with_temps(state.current_temp, state.target_temp);
            }
        }

        // 4. Normal operation
        SafetyCheckResult::normal("temperature", format!("heater/{}", state.name))
            .with_temps(state.current_temp, state.target_temp)
    }

    /// Detect sensor fault based on absolute thresholds
    fn detect_sensor_fault(&self, state: &HeaterReading) -> bool {
        // Check per-heater config first
        if let Some(config) = self.heater_configs.get(&state.name) {
            return state.current_temp > config.sensor_fault.max_temp
                || state.current_temp < config.sensor_fault.min_temp;
        }
        // Fallback to global thresholds
        state.current_temp > self.sensor_fault_max_temp
            || state.current_temp < self.sensor_fault_min_temp
    }

    /// Determine action for low temperature
    fn get_low_temp_action(
        &self,
        heater_name: &str,
        heating_duration: f64,
        heater_config: Option<&HeaterSafetyConfig>,
    ) -> (SafetyLevel, SafetyAction) {
        if let Some(config) = heater_config {
            let level = if heating_duration > config.heating_delay_secs as f64 {
                SafetyLevel::Critical
            } else {
                SafetyLevel::Warning
            };

            let action_str = match level {
                SafetyLevel::Warning => &config.low_temp.warning,
                SafetyLevel::Critical => &config.low_temp.critical,
                SafetyLevel::Emergency => &config.low_temp.emergency,
                _ => "none",
            };

            return (level, self.parse_action(action_str));
        }

        // Default behavior
        match heater_name {
            "bed" => {
                if heating_duration > 120.0 {
                    (SafetyLevel::Warning, SafetyAction::LogWarning)
                } else {
                    (SafetyLevel::Normal, SafetyAction::None)
                }
            }
            "hotend" => {
                if heating_duration > 60.0 {
                    (SafetyLevel::Critical, SafetyAction::PausePrint)
                } else {
                    (SafetyLevel::Warning, SafetyAction::LogWarning)
                }
            }
            _ => (SafetyLevel::Warning, SafetyAction::LogWarning),
        }
    }

    /// Determine action for high temperature
    fn get_high_temp_action(
        &self,
        heater_name: &str,
        heater_config: Option<&HeaterSafetyConfig>,
    ) -> (SafetyLevel, SafetyAction) {
        if let Some(config) = heater_config {
            let level = SafetyLevel::Critical;
            let action_str = match level {
                SafetyLevel::Warning => &config.high_temp.warning,
                SafetyLevel::Critical => &config.high_temp.critical,
                SafetyLevel::Emergency => &config.high_temp.emergency,
                _ => "none",
            };
            return (level, self.parse_action(action_str));
        }

        // Default
        match heater_name {
            "bed" => (SafetyLevel::Critical, SafetyAction::TurnOffHeater),
            "hotend" => (SafetyLevel::Critical, SafetyAction::TurnOffHeater),
            _ => (SafetyLevel::Warning, SafetyAction::LogWarning),
        }
    }

    /// Parse action string to SafetyAction enum
    fn parse_action(&self, action_str: &str) -> SafetyAction {
        match action_str {
            "warn" => SafetyAction::LogWarning,
            "pause_print" => SafetyAction::PausePrint,
            "turn_off" => SafetyAction::TurnOffHeater,
            "emergency_stop" => SafetyAction::EmergencyStop,
            _ => SafetyAction::None,
        }
    }

    /// Check multiple heaters
    pub fn check_heaters(&self, heaters: &[HeaterReading]) -> Vec<SafetyCheckResult> {
        heaters.iter().map(|h| self.check_heater(h)).collect()
    }

    /// Filter results that need action
    pub fn filter_action_needed(results: &[SafetyCheckResult]) -> Vec<&SafetyCheckResult> {
        results.iter().filter(|r| r.needs_action()).collect()
    }

    /// Get the most critical result
    pub fn get_most_critical<'a>(results: &'a [SafetyCheckResult]) -> Option<&'a SafetyCheckResult> {
        results.iter().max_by_key(|r| r.level.rank())
    }
}

impl Default for TemperatureSafetyChecker {
    fn default() -> Self {
        Self::new(&SafetyConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_reading(name: &str, current: f32, target: f32) -> HeaterReading {
        HeaterReading {
            name: name.to_string(),
            current_temp: current,
            target_temp: target,
            is_heating: target > 0.0,
            heating_duration_secs: 10.0,
            min_temp: 0.0,
            max_temp: 250.0,
            sensor_fault: false,
        }
    }

    #[test]
    fn test_normal_temperature() {
        let checker = TemperatureSafetyChecker::default();
        let reading = make_reading("hotend", 200.0, 200.0);
        let result = checker.check_heater(&reading);
        assert!(result.passed);
    }

    #[test]
    fn test_sensor_fault() {
        let checker = TemperatureSafetyChecker::default();
        let mut reading = make_reading("hotend", 999.0, 200.0);
        reading.sensor_fault = true;
        let result = checker.check_heater(&reading);
        assert!(!result.passed);
        assert_eq!(result.level, SafetyLevel::Emergency);
    }

    #[test]
    fn test_temperature_below_min() {
        let checker = TemperatureSafetyChecker::default();
        let reading = make_reading("hotend", -10.0, 200.0);
        let result = checker.check_heater(&reading);
        assert!(!result.passed);
        assert_eq!(result.level, SafetyLevel::Emergency);
    }

    #[test]
    fn test_temperature_above_max() {
        let checker = TemperatureSafetyChecker::default();
        let reading = make_reading("hotend", 300.0, 200.0);
        let result = checker.check_heater(&reading);
        assert!(!result.passed);
        assert_eq!(result.level, SafetyLevel::Emergency);
    }

    #[test]
    fn test_deviation_high_critical() {
        let checker = TemperatureSafetyChecker::default();
        let reading = make_reading("hotend", 220.0, 200.0);
        let result = checker.check_heater(&reading);
        assert!(!result.passed);
        assert_eq!(result.level, SafetyLevel::Critical);
        assert_eq!(result.action, SafetyAction::PausePrint);
    }

    #[test]
    fn test_deviation_low_critical() {
        let checker = TemperatureSafetyChecker::default();
        let mut reading = make_reading("hotend", 180.0, 200.0);
        reading.heating_duration_secs = 120.0; // past heating delay
        let result = checker.check_heater(&reading);
        assert!(!result.passed);
        assert_eq!(result.level, SafetyLevel::Critical);
        assert_eq!(result.action, SafetyAction::PausePrint);
    }
}