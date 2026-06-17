//! Hardware safety event handler
//!
//! This module handles hardware-level safety events such as:
//! - Filament runout detection (pause print)
//! - Power loss detection (emergency stop)
//! - Device state staleness (warning / emergency)

use super::types::{SafetyAction, SafetyCheckResult, SafetyLevel};

/// Types of hardware events that can trigger safety actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardwareEvent {
    /// Filament runout detected
    FilamentRunout,
    /// Power loss detected
    PowerLoss,
    /// Device state is stale (no update received for too long)
    StateStale,
    /// Motor error (stall, over-current, etc.)
    MotorError,
}

impl HardwareEvent {
    /// Event name for logging
    pub fn name(&self) -> &'static str {
        match self {
            Self::FilamentRunout => "filament_runout",
            Self::PowerLoss => "power_loss",
            Self::StateStale => "state_stale",
            Self::MotorError => "motor_error",
        }
    }
}

/// Hardware safety checker
///
/// Handles GPIO-triggered hardware events and converts them
/// into structured safety check results.
pub struct HardwareSafetyChecker {
    /// Whether hardware safety checks are enabled
    enabled: bool,

    /// Maximum time (milliseconds) without state update before considering stale
    state_stale_threshold_ms: u64,
}

impl HardwareSafetyChecker {
    /// Create a new hardware safety checker
    pub fn new(enabled: bool, state_stale_threshold_ms: u64) -> Self {
        Self {
            enabled,
            state_stale_threshold_ms,
        }
    }

    /// Handle a hardware event and return the corresponding safety result
    pub fn handle_event(&self, event: HardwareEvent) -> SafetyCheckResult {
        if !self.enabled {
            return SafetyCheckResult::normal("hardware", format!("event/{}", event.name()));
        }

        match event {
            HardwareEvent::FilamentRunout => SafetyCheckResult {
                source: "hardware".to_string(),
                check_name: "event/filament_runout".to_string(),
                passed: false,
                level: SafetyLevel::Warning,
                action: SafetyAction::PausePrint,
                message: "Filament runout detected".to_string(),
                current_temp: None,
                target_temp: None,
            },
            HardwareEvent::PowerLoss => SafetyCheckResult {
                source: "hardware".to_string(),
                check_name: "event/power_loss".to_string(),
                passed: false,
                level: SafetyLevel::Emergency,
                action: SafetyAction::EmergencyStop,
                message: "Power loss detected".to_string(),
                current_temp: None,
                target_temp: None,
            },
            HardwareEvent::StateStale => SafetyCheckResult {
                source: "hardware".to_string(),
                check_name: "event/state_stale".to_string(),
                passed: false,
                level: SafetyLevel::Warning,
                action: SafetyAction::LogWarning,
                message: "Device state is stale — no recent updates".to_string(),
                current_temp: None,
                target_temp: None,
            },
            HardwareEvent::MotorError => SafetyCheckResult {
                source: "hardware".to_string(),
                check_name: "event/motor_error".to_string(),
                passed: false,
                level: SafetyLevel::Critical,
                action: SafetyAction::DisableMotors,
                message: "Motor error detected (possible stall or over-current)".to_string(),
                current_temp: None,
                target_temp: None,
            },
        }
    }

    /// Check if device state is stale based on elapsed time
    pub fn check_state_staleness(&self, elapsed_ms: u64) -> Option<SafetyCheckResult> {
        if !self.enabled {
            return None;
        }

        if elapsed_ms > self.state_stale_threshold_ms {
            Some(self.handle_event(HardwareEvent::StateStale))
        } else {
            None
        }
    }

    /// Reload configuration
    pub fn reload(&mut self, enabled: bool, state_stale_threshold_ms: u64) {
        self.enabled = enabled;
        self.state_stale_threshold_ms = state_stale_threshold_ms;
    }

    /// Execute a hardware event (fire-and-forget style)
    /// Returns the safety result for downstream processing
    pub fn trigger(&self, event: HardwareEvent) -> SafetyCheckResult {
        let result = self.handle_event(event);
        log::warn!(
            "Hardware safety event: {} (level={:?}, action={:?})",
            event.name(),
            result.level,
            result.action,
        );
        result
    }
}

impl Default for HardwareSafetyChecker {
    fn default() -> Self {
        Self::new(true, 5000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filament_runout() {
        let checker = HardwareSafetyChecker::default();
        let result = checker.handle_event(HardwareEvent::FilamentRunout);
        assert!(!result.passed);
        assert_eq!(result.action, SafetyAction::PausePrint);
    }

    #[test]
    fn test_power_loss() {
        let checker = HardwareSafetyChecker::default();
        let result = checker.handle_event(HardwareEvent::PowerLoss);
        assert!(!result.passed);
        assert_eq!(result.action, SafetyAction::EmergencyStop);
        assert_eq!(result.level, SafetyLevel::Emergency);
    }

    #[test]
    fn test_state_staleness() {
        let checker = HardwareSafetyChecker::new(true, 5000);
        assert!(checker.check_state_staleness(1000).is_none());
        assert!(checker.check_state_staleness(6000).is_some());
    }

    #[test]
    fn test_disabled_checker() {
        let checker = HardwareSafetyChecker::new(false, 5000);
        let result = checker.handle_event(HardwareEvent::PowerLoss);
        assert!(result.passed);
    }
}