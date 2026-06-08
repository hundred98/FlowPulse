//! Temperature safety checker
//!
//! This module provides safety checking for temperature management,
//! including temperature deviation detection and safety action determination.

use super::types::{HeaterState, SafetyAction, SafetyCheckResult, SafetyLevel, TemperatureManagerConfig};

/// Temperature safety checker
pub struct TemperatureSafetyChecker {
    /// Configuration
    config: TemperatureManagerConfig,
}

impl TemperatureSafetyChecker {
    /// Create a new safety checker
    pub fn new(config: TemperatureManagerConfig) -> Self {
        Self { config }
    }

    /// Check a single heater
    pub fn check_heater(&self, state: &HeaterState) -> SafetyCheckResult {
        let deviation = state.deviation();

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

        // 3. Check temperature deviation from target
        if deviation < -self.config.max_deviation_emergency {
            // Temperature too low (emergency level)
            let (level, action) = self.get_low_temp_action(&state.name);
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

        if deviation > self.config.max_deviation_emergency {
            // Temperature too high (emergency level)
            return SafetyCheckResult::dangerous(
                state.name.clone(),
                format!(
                    "Temperature too high: {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                    state.current_temp, state.target_temp, deviation
                ),
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        if deviation > self.config.max_deviation_critical {
            // Temperature high (critical level)
            return SafetyCheckResult::critical(
                state.name.clone(),
                format!(
                    "Temperature high: {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                    state.current_temp, state.target_temp, deviation
                ),
            )
            .with_temps(state.current_temp, state.target_temp);
        }

        if deviation < -self.config.max_deviation_warning {
            // Temperature low (warning level)
            let (level, action) = self.get_low_temp_action(&state.name);
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

        if deviation > self.config.max_deviation_warning {
            // Temperature slightly high (warning level)
            return SafetyCheckResult::warning(
                state.name.clone(),
                format!(
                    "Temperature slightly high: {:.1}°C (target: {:.1}°C, deviation: {:.1}°C)",
                    state.current_temp, state.target_temp, deviation
                ),
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

    /// Get action for low temperature based on heater type
    fn get_low_temp_action(&self, heater_name: &str) -> (SafetyLevel, SafetyAction) {
        match heater_name {
            "bed" => {
                // Bed temperature drop is less critical
                (SafetyLevel::Warning, SafetyAction::Warn)
            }
            "hotend" => {
                // Hotend temperature drop is critical (need to pause print)
                (SafetyLevel::Critical, SafetyAction::PausePrint)
            }
            _ => {
                // Other heaters: warning
                (SafetyLevel::Warning, SafetyAction::Warn)
            }
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
        Self::new(TemperatureManagerConfig::default())
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
