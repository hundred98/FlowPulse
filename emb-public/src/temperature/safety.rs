//! Temperature safety checker
//!
//! This module provides safety checking for temperature management,
//! including temperature deviation detection and safety action determination.

use super::types::{HeaterState, SafetyAction, SafetyCheckResult, SafetyLevel};
use crate::config::{TempHeaterSafetyConfig, TemperatureSafetyConfig};
use std::collections::HashMap;

/// Temperature safety checker
pub struct TemperatureSafetyChecker {
    /// Per-heater safety configuration
    heater_configs: HashMap<String, TempHeaterSafetyConfig>,
}

impl TemperatureSafetyChecker {
    /// Create a new safety checker
    pub fn new(config: TemperatureSafetyConfig) -> Self {
        let heater_configs = config.heaters.clone();
        Self {
            heater_configs,
        }
    }

    /// Check a single heater
    pub fn check_heater(&self, state: &HeaterState) -> SafetyCheckResult {
        // 0. Check for sensor fault first
        if state.has_sensor_fault() {
            return SafetyCheckResult::dangerous(
                state.name.clone(),
                format!(
                    "Sensor fault detected: {:.1}°C (abnormal value)",
                    state.current_temp
                ),
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        // 1. Check if temperature is below minimum
        if state.current_temp < state.min_temp {
            return SafetyCheckResult::dangerous(
                state.name.clone(),
                format!(
                    "Temperature below minimum: {:.1}°C < {:.1}°C",
                    state.current_temp, state.min_temp
                ),
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        // 2. Check if temperature is above maximum
        if state.current_temp > state.max_temp {
            return SafetyCheckResult::dangerous(
                state.name.clone(),
                format!(
                    "Temperature above maximum: {:.1}°C > {:.1}°C",
                    state.current_temp, state.max_temp
                ),
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        let deviation = state.deviation();

        // 3. Check temperature deviation from target
        // Get heater-specific configuration
        let heater_config = self.heater_configs.get(&state.name);
        let heating_delay_secs = heater_config
            .map(|c| c.heating_delay_secs as f64)
            .unwrap_or(60.0);

        // During heating, allow more deviation for the initial period
        let heating_duration = state.heating_duration_secs();
        let is_heating_up = state.is_heating && deviation < 0.0 && heating_duration < heating_delay_secs;

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

        // Skip low temperature checks during initial heating phase
        if !is_heating_up {
            if deviation < -emergency_threshold {
                // Temperature too low (emergency level)
                let (level, action) = self.get_low_temp_action(&state.name, heating_duration, heater_config);
                return SafetyCheckResult::new(
                    state.name.clone(),
                    level,
                    format!(
                        "Temperature too low: {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                        state.current_temp, state.target_temp, deviation
                    ),
                    action,
                )
                .with_temps(state.current_temp, state.target_temp);
            }

            if deviation < -critical_threshold {
                // Temperature low (critical level)
                let (level, action) = self.get_low_temp_action(&state.name, heating_duration, heater_config);
                return SafetyCheckResult::new(
                    state.name.clone(),
                    level,
                    format!(
                        "Temperature low: {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                        state.current_temp, state.target_temp, deviation
                    ),
                    action,
                )
                .with_temps(state.current_temp, state.target_temp);
            }

            if deviation < -warning_threshold {
                // Temperature slightly low (warning level)
                let (level, action) = self.get_low_temp_action(&state.name, heating_duration, heater_config);
                return SafetyCheckResult::new(
                    state.name.clone(),
                    level,
                    format!(
                        "Temperature slightly low: {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                        state.current_temp, state.target_temp, deviation
                    ),
                    action,
                )
                .with_temps(state.current_temp, state.target_temp);
            }
        }

        // Check for high temperature (always check, regardless of heating state)
        if deviation > emergency_threshold {
            // Temperature too high (emergency level)
            let (level, action) = self.get_high_temp_action(&state.name, heater_config);
            return SafetyCheckResult::new(
                state.name.clone(),
                level,
                format!(
                    "Temperature too high: {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                    state.current_temp, state.target_temp, deviation
                ),
                action,
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        if deviation > critical_threshold {
            // Temperature high (critical level)
            let (level, action) = self.get_high_temp_action(&state.name, heater_config);
            return SafetyCheckResult::new(
                state.name.clone(),
                level,
                format!(
                    "Temperature high: {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                    state.current_temp, state.target_temp, deviation
                ),
                action,
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        if deviation > warning_threshold {
            // Temperature slightly high (warning level)
            let (level, action) = self.get_high_temp_action(&state.name, heater_config);
            return SafetyCheckResult::new(
                state.name.clone(),
                level,
                format!(
                    "Temperature slightly high: {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                    state.current_temp, state.target_temp, deviation
                ),
                action,
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        // 4. Check if heater is off but temperature is rising
        if !state.is_heating && deviation > 5.0 {
            return SafetyCheckResult::dangerous(
                state.name.clone(),
                format!(
                    "Temperature rising while heater off: {:.1}°C",
                    state.current_temp
                ),
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        // 5. Normal operation
        SafetyCheckResult::normal(state.name.clone())
            .with_temps(state.current_temp, state.target_temp)
    }

    /// Get action for low temperature based on heater configuration
    fn get_low_temp_action(
        &self,
        heater_name: &str,
        heating_duration: f64,
        heater_config: Option<&TempHeaterSafetyConfig>,
    ) -> (SafetyLevel, SafetyAction) {
        // If heater has specific configuration, use it
        if let Some(config) = heater_config {
            // Determine level based on heating duration
            let level = if heating_duration > config.heating_delay_secs as f64 {
                SafetyLevel::Critical
            } else {
                SafetyLevel::Warning
            };

            // Get action based on level
            let action_str = match level {
                SafetyLevel::Warning => &config.actions.low_temp.warning,
                SafetyLevel::Critical => &config.actions.low_temp.critical,
                SafetyLevel::Dangerous => &config.actions.low_temp.emergency,
                SafetyLevel::Normal => "none",
            };

            return (level, self.parse_action(action_str));
        }

        // Default behavior for heaters without specific configuration
        match heater_name {
            "bed" => {
                // Bed temperature issues are less critical
                if heating_duration > 120.0 {
                    (SafetyLevel::Warning, SafetyAction::Warn)
                } else {
                    (SafetyLevel::Normal, SafetyAction::None)
                }
            }
            "hotend" => {
                // Hotend temperature issues are more critical
                if heating_duration > 60.0 {
                    (SafetyLevel::Critical, SafetyAction::PausePrint)
                } else {
                    (SafetyLevel::Warning, SafetyAction::Warn)
                }
            }
            _ => {
                // Other heaters: warning
                (SafetyLevel::Warning, SafetyAction::Warn)
            }
        }
    }

    /// Get action for high temperature based on heater configuration
    fn get_high_temp_action(
        &self,
        heater_name: &str,
        heater_config: Option<&TempHeaterSafetyConfig>,
    ) -> (SafetyLevel, SafetyAction) {
        // If heater has specific configuration, use it
        if let Some(config) = heater_config {
            // High temperature is always critical
            let level = SafetyLevel::Critical;

            // Get action based on level
            let action_str = match level {
                SafetyLevel::Warning => &config.actions.high_temp.warning,
                SafetyLevel::Critical => &config.actions.high_temp.critical,
                SafetyLevel::Dangerous => &config.actions.high_temp.emergency,
                SafetyLevel::Normal => "none",
            };

            return (level, self.parse_action(action_str));
        }

        // Default behavior for heaters without specific configuration
        match heater_name {
            "bed" => (SafetyLevel::Critical, SafetyAction::TurnOffHeater),
            "hotend" => (SafetyLevel::Critical, SafetyAction::TurnOffHeater),
            _ => (SafetyLevel::Warning, SafetyAction::Warn),
        }
    }

    /// Parse action string to SafetyAction enum
    fn parse_action(&self, action_str: &str) -> SafetyAction {
        match action_str {
            "warn" => SafetyAction::Warn,
            "pause_print" => SafetyAction::PausePrint,
            "turn_off" => SafetyAction::TurnOffHeater,
            "emergency_stop" => SafetyAction::EmergencyStop,
            _ => SafetyAction::None,
        }
    }

    /// Check multiple heaters
    pub fn check_heaters(&self, heaters: &[HeaterState]) -> Vec<SafetyCheckResult> {
        heaters.iter().map(|h| self.check_heater(h)).collect()
    }

    /// Filter results that need action
    pub fn filter_action_needed(results: Vec<SafetyCheckResult>) -> Vec<SafetyCheckResult> {
        results.into_iter().filter(|r| r.needs_action()).collect()
    }

    /// Get the most critical result
    pub fn get_most_critical(results: &[SafetyCheckResult]) -> Option<&SafetyCheckResult> {
        results.iter().max_by_key(|r| match r.level {
            SafetyLevel::Normal => 0,
            SafetyLevel::Warning => 1,
            SafetyLevel::Critical => 2,
            SafetyLevel::Dangerous => 3,
        })
    }
}

impl Default for TemperatureSafetyChecker {
    fn default() -> Self {
        Self::new(TemperatureSafetyConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn create_heater(name: &str, current: f32, target: f32) -> HeaterState {
        let mut heater = HeaterState::new(name.to_string(), 1, 0.0, 250.0);
        heater.current_temp = current;
        heater.target_temp = target;
        heater.is_heating = target > 0.0;
        heater.last_update = Instant::now();
        heater
    }

    #[test]
    fn test_normal_temperature() {
        let checker = TemperatureSafetyChecker::default();
        let heater = create_heater("hotend", 200.0, 200.0);

        let result = checker.check_heater(&heater);

        assert_eq!(result.level, SafetyLevel::Normal);
        assert_eq!(result.action, SafetyAction::None);
        assert!(!result.needs_action());
    }

    #[test]
    fn test_temperature_low_warning() {
        let checker = TemperatureSafetyChecker::default();
        let heater = create_heater("hotend", 190.0, 200.0);

        let result = checker.check_heater(&heater);

        assert_eq!(result.level, SafetyLevel::Critical);
        assert_eq!(result.action, SafetyAction::PausePrint);
        assert!(result.needs_action());
    }

    #[test]
    fn test_temperature_high_warning() {
        let checker = TemperatureSafetyChecker::default();
        let heater = create_heater("hotend", 205.0, 200.0);

        let result = checker.check_heater(&heater);

        assert_eq!(result.level, SafetyLevel::Warning);
        assert_eq!(result.action, SafetyAction::Warn);
        assert!(result.needs_action());
    }

    #[test]
    fn test_temperature_high_critical() {
        let checker = TemperatureSafetyChecker::default();
        let heater = create_heater("hotend", 220.0, 200.0);

        let result = checker.check_heater(&heater);

        assert_eq!(result.level, SafetyLevel::Critical);
        assert_eq!(result.action, SafetyAction::PausePrint);
    }

    #[test]
    fn test_temperature_high_dangerous() {
        let checker = TemperatureSafetyChecker::default();
        let heater = create_heater("hotend", 230.0, 200.0);

        let result = checker.check_heater(&heater);

        assert_eq!(result.level, SafetyLevel::Dangerous);
        assert_eq!(result.action, SafetyAction::EmergencyStop);
    }

    #[test]
    fn test_bed_temperature_low() {
        let checker = TemperatureSafetyChecker::default();
        let heater = create_heater("bed", 50.0, 60.0);

        let result = checker.check_heater(&heater);

        // Bed temperature drop should be warning, not critical
        assert_eq!(result.level, SafetyLevel::Warning);
        assert_eq!(result.action, SafetyAction::Warn);
    }

    #[test]
    fn test_heater_off_but_rising() {
        let checker = TemperatureSafetyChecker::default();
        let mut heater = create_heater("hotend", 210.0, 200.0);
        heater.is_heating = false;

        let result = checker.check_heater(&heater);

        assert_eq!(result.level, SafetyLevel::Dangerous);
        assert_eq!(result.action, SafetyAction::EmergencyStop);
    }

    #[test]
    fn test_get_most_critical() {
        let results = vec![
            SafetyCheckResult::normal("bed".to_string()),
            SafetyCheckResult::warning("hotend".to_string(), "Warning".to_string()),
            SafetyCheckResult::critical("chamber".to_string(), "Critical".to_string()),
        ];

        let most_critical = TemperatureSafetyChecker::get_most_critical(&results);

        assert!(most_critical.is_some());
        let most_critical = most_critical.unwrap();
        assert_eq!(most_critical.heater, "chamber");
        assert_eq!(most_critical.level, SafetyLevel::Critical);
    }
}
