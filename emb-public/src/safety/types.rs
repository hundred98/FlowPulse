//! Unified safety types for the safety module
//!
//! This module defines all shared types used across the safety subsystem,
//! consolidating previously duplicated types from temperature and safety modules.

/// Safety check level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyLevel {
    /// Normal operation — no issue
    Normal,

    /// Warning — minor deviation, log and notify
    Warning,

    /// Critical — requires pausing print or turning off heater
    Critical,

    /// Emergency — requires emergency stop
    Emergency,
}

impl Default for SafetyLevel {
    fn default() -> Self {
        Self::Normal
    }
}

impl SafetyLevel {
    /// Rank for severity comparison (higher = more severe)
    pub fn rank(&self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Warning => 1,
            Self::Critical => 2,
            Self::Emergency => 3,
        }
    }

    /// Check if this level requires an action
    pub fn needs_action(&self) -> bool {
        matches!(self, Self::Warning | Self::Critical | Self::Emergency)
    }

    /// Check if this is a critical or emergency level
    pub fn is_severe(&self) -> bool {
        matches!(self, Self::Critical | Self::Emergency)
    }
}

/// Safety action to take
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyAction {
    /// No action needed
    None,

    /// Publish warning event / log
    LogWarning,

    /// Turn off the affected heater
    TurnOffHeater,

    /// Pause the current print job
    PausePrint,

    /// Disable all motors
    DisableMotors,

    /// Emergency stop (all heaters off, motors off, error state)
    EmergencyStop,
}

impl Default for SafetyAction {
    fn default() -> Self {
        Self::None
    }
}

/// Unified safety check result
///
/// This replaces both `safety::SafetyCheckResult` (motion checks)
/// and `temperature::types::SafetyCheckResult` (temperature checks).
#[derive(Debug, Clone)]
pub struct SafetyCheckResult {
    /// Source subsystem: "motion", "temperature", "hardware", etc.
    pub source: String,

    /// Specific check name (e.g. "motion_limits", "sensor_fault", "deviation")
    pub check_name: String,

    /// Whether the check passed
    pub passed: bool,

    /// Safety level
    pub level: SafetyLevel,

    /// Suggested action
    pub action: SafetyAction,

    /// Human-readable description
    pub message: String,

    /// Current temperature (if applicable)
    pub current_temp: Option<f32>,

    /// Target temperature (if applicable)
    pub target_temp: Option<f32>,
}

impl SafetyCheckResult {
    /// Create a new result
    pub fn new(
        source: impl Into<String>,
        check_name: impl Into<String>,
        level: SafetyLevel,
        action: SafetyAction,
        message: impl Into<String>,
    ) -> Self {
        let passed = matches!(level, SafetyLevel::Normal);
        Self {
            source: source.into(),
            check_name: check_name.into(),
            passed,
            level,
            action,
            message: message.into(),
            current_temp: None,
            target_temp: None,
        }
    }

    /// Create a normal (passed) result
    pub fn normal(source: impl Into<String>, check_name: impl Into<String>) -> Self {
        Self::new(source, check_name, SafetyLevel::Normal, SafetyAction::None, "OK")
    }

    /// Create a warning result
    pub fn warning(
        source: impl Into<String>,
        check_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(source, check_name, SafetyLevel::Warning, SafetyAction::LogWarning, message)
    }

    /// Create a critical result
    pub fn critical(
        source: impl Into<String>,
        check_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(source, check_name, SafetyLevel::Critical, SafetyAction::PausePrint, message)
    }

    /// Create an emergency result
    pub fn emergency(
        source: impl Into<String>,
        check_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(source, check_name, SafetyLevel::Emergency, SafetyAction::EmergencyStop, message)
    }

    /// Attach temperature data
    pub fn with_temps(mut self, current: f32, target: f32) -> Self {
        self.current_temp = Some(current);
        self.target_temp = Some(target);
        self
    }

    /// Check if action is needed
    pub fn needs_action(&self) -> bool {
        self.level.needs_action()
    }

    /// Check if this is severe (critical or emergency)
    pub fn is_severe(&self) -> bool {
        self.level.is_severe()
    }
}

/// Compare two results by severity (for finding the most critical)
impl Eq for SafetyCheckResult {}

impl PartialEq for SafetyCheckResult {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && self.check_name == other.check_name
            && self.level == other.level
    }
}

impl PartialOrd for SafetyCheckResult {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SafetyCheckResult {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.level.rank().cmp(&other.level.rank())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safety_level_ranking() {
        assert!(SafetyLevel::Emergency.rank() > SafetyLevel::Critical.rank());
        assert!(SafetyLevel::Critical.rank() > SafetyLevel::Warning.rank());
        assert!(SafetyLevel::Warning.rank() > SafetyLevel::Normal.rank());
    }

    #[test]
    fn test_safety_check_result_creation() {
        let r = SafetyCheckResult::normal("motion", "x_limit");
        assert!(r.passed);
        assert_eq!(r.level, SafetyLevel::Normal);
        assert_eq!(r.action, SafetyAction::None);
        assert!(!r.needs_action());

        let r = SafetyCheckResult::emergency("temperature", "sensor_fault", "Sensor fault detected")
            .with_temps(350.0, 200.0);
        assert!(!r.passed);
        assert!(r.needs_action());
        assert_eq!(r.current_temp, Some(350.0));
        assert_eq!(r.target_temp, Some(200.0));
    }

    #[test]
    fn test_result_ordering() {
        let normal = SafetyCheckResult::normal("test", "check");
        let warning = SafetyCheckResult::warning("test", "check", "msg");
        let critical = SafetyCheckResult::critical("test", "check", "msg");
        let emergency = SafetyCheckResult::emergency("test", "check", "msg");

        assert!(warning > normal);
        assert!(critical > warning);
        assert!(emergency > critical);
    }
}