//! Motion safety checker
//!
//! This module provides safety checks related to motion:
//! - Position limit checks (X/Y/Z axis bounds)
//! - Endstop/limit switch status monitoring (future)

use super::config::SafetyConfig;
use super::types::{SafetyAction, SafetyCheckResult, SafetyLevel};
use std::collections::HashMap;

/// Motion safety checker
///
/// Checks axis positions against configured motion limits.
/// This is a lightweight, stateless checker that operates on position data.
pub struct MotionSafetyChecker {
    /// Motion limits for axes (keyed by axis name: "x", "y", "z")
    motion_limits: HashMap<String, super::config::MotionLimit>,

    /// Whether checking is enabled
    enabled: bool,
}

impl MotionSafetyChecker {
    /// Create a new motion safety checker from config
    pub fn new(config: &SafetyConfig) -> Self {
        Self {
            motion_limits: config.motion_limits.clone(),
            enabled: config.enable_safety_checks,
        }
    }

    /// Reload configuration
    pub fn reload(&mut self, config: &SafetyConfig) {
        self.motion_limits = config.motion_limits.clone();
        self.enabled = config.enable_safety_checks;
    }

    /// Check a single axis position against its limits
    pub fn check_axis(&self, axis: &str, position: f32) -> SafetyCheckResult {
        if !self.enabled {
            return SafetyCheckResult::normal("motion", format!("axis_{}_limit", axis));
        }

        let limit = match self.motion_limits.get(axis) {
            Some(l) => l,
            None => return SafetyCheckResult::normal("motion", format!("axis_{}_limit", axis)),
        };

        if position < limit.min {
            return SafetyCheckResult {
                source: "motion".to_string(),
                check_name: format!("axis_{}_limit", axis),
                passed: false,
                level: SafetyLevel::Warning,
                action: SafetyAction::LogWarning,
                message: format!(
                    "Axis '{}' position {:.1} mm below minimum {:.1} mm",
                    axis, position, limit.min
                ),
                current_temp: None,
                target_temp: None,
            };
        }

        if position > limit.max {
            return SafetyCheckResult {
                source: "motion".to_string(),
                check_name: format!("axis_{}_limit", axis),
                passed: false,
                level: SafetyLevel::Critical,
                action: SafetyAction::PausePrint,
                message: format!(
                    "Axis '{}' position {:.1} mm exceeds maximum {:.1} mm",
                    axis, position, limit.max
                ),
                current_temp: None,
                target_temp: None,
            };
        }

        SafetyCheckResult::normal("motion", format!("axis_{}_limit", axis))
    }

    /// Check all axes
    pub fn check_all_axes(
        &self,
        axes: &[(&str, f32)],
    ) -> Vec<SafetyCheckResult> {
        axes.iter()
            .map(|(name, pos)| self.check_axis(name, *pos))
            .collect()
    }
}

impl Default for MotionSafetyChecker {
    fn default() -> Self {
        Self::new(&SafetyConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_axis_within_limits() {
        let checker = MotionSafetyChecker::default();
        let result = checker.check_axis("x", 150.0);
        assert!(result.passed);
    }

    #[test]
    fn test_axis_below_min() {
        let checker = MotionSafetyChecker::default();
        let result = checker.check_axis("x", -5.0);
        assert!(!result.passed);
        assert_eq!(result.level, SafetyLevel::Warning);
    }

    #[test]
    fn test_axis_above_max() {
        let checker = MotionSafetyChecker::default();
        let result = checker.check_axis("x", 350.0);
        assert!(!result.passed);
        assert_eq!(result.level, SafetyLevel::Critical);
    }

    #[test]
    fn test_disabled_checker() {
        let mut config = SafetyConfig::default();
        config.enable_safety_checks = false;
        let checker = MotionSafetyChecker::new(&config);
        let result = checker.check_axis("x", -999.0);
        assert!(result.passed);
    }

    #[test]
    fn test_unknown_axis() {
        let checker = MotionSafetyChecker::default();
        let result = checker.check_axis("e", 100.0);
        assert!(result.passed);
    }
}