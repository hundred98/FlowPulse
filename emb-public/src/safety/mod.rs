//! Safety controller module
//!
//! This module provides a unified safety management system for the printer,
//! consolidating motion limits, temperature safety, hardware event handling,
//! and emergency stop management into a single subsystem.
//!
//! Sub-modules:
//! - `types`       — Unified safety types (SafetyLevel, SafetyAction, SafetyCheckResult)
//! - `config`      — Safety configuration (motion limits, temperature thresholds)
//! - `actions`     — Safety action executor (turn off heater, pause print, emergency stop)
//! - `motion`      — Motion safety checker (position limits)
//! - `temperature` — Temperature safety checker (sensor fault, deviation)
//! - `hardware`    — Hardware safety event handler (filament runout, power loss)
//! - `monitor`     — Periodic safety check loop

pub mod types;
pub mod config;
pub mod actions;
pub mod motion;
pub mod temperature;
pub mod hardware;
pub mod monitor;

pub use types::{SafetyLevel, SafetyAction, SafetyCheckResult};
pub use config::{SafetyConfig, MotionLimit, TemperatureLimit};
pub use actions::SafetyActionExecutor;
pub use motion::MotionSafetyChecker;
pub use temperature::{TemperatureSafetyChecker, HeaterReading};
pub use hardware::{HardwareSafetyChecker, HardwareEvent};
pub use monitor::{SafetyMonitor, SafetyDataSource};

use crate::common::{EmbResult, EventPublisher, EventKind, EventSeverity, PrinterEvent};
use crate::state::{DeviceStateManager, DeviceStateConfig};
use std::sync::Arc;

/// Safety controller
///
/// Central entry point for all safety-related operations.
/// Manages emergency stop, coordinates safety checkers, and dispatches safety actions.
pub struct SafetyController {
    /// Safety configuration
    config: std::sync::RwLock<SafetyConfig>,

    /// Device state manager (for position data)
    device_state: Arc<DeviceStateManager>,

    /// Event publisher (for publishing safety events)
    event_publisher: Arc<dyn EventPublisher>,

    /// Emergency stop flag
    emergency_stop_active: tokio::sync::RwLock<bool>,

    /// Sub-checkers (optional, created on demand)
    motion_checker: std::sync::RwLock<Option<MotionSafetyChecker>>,
    temperature_checker: std::sync::RwLock<Option<TemperatureSafetyChecker>>,
    hardware_checker: std::sync::RwLock<Option<HardwareSafetyChecker>>,
}

impl SafetyController {
    /// Create a new safety controller
    pub fn new(
        config: SafetyConfig,
        device_state: Arc<DeviceStateManager>,
        event_publisher: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            config: std::sync::RwLock::new(config),
            device_state,
            event_publisher,
            emergency_stop_active: tokio::sync::RwLock::new(false),
            motion_checker: std::sync::RwLock::new(None),
            temperature_checker: std::sync::RwLock::new(None),
            hardware_checker: std::sync::RwLock::new(None),
        }
    }

    // ---- Emergency stop ----

    /// Handle emergency stop
    pub async fn handle_emergency_stop(&self) -> EmbResult<()> {
        let mut emergency_stop = self.emergency_stop_active.write().await;
        *emergency_stop = true;

        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "safety".to_string(),
            "Emergency stop activated".to_string(),
        ).with_severity(EventSeverity::Critical));

        log::warn!("Emergency stop activated");
        Ok(())
    }

    /// Check if emergency stop is active
    pub async fn is_emergency_stop_active(&self) -> bool {
        *self.emergency_stop_active.read().await
    }

    /// Clear emergency stop
    pub async fn clear_emergency_stop(&self) -> EmbResult<()> {
        let mut emergency_stop = self.emergency_stop_active.write().await;
        *emergency_stop = false;

        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "safety".to_string(),
            "Emergency stop cleared".to_string(),
        ).with_severity(EventSeverity::Info));

        log::info!("Emergency stop cleared");
        Ok(())
    }

    // ---- Motion checks ----

    /// Check motion limits for a single axis (backward-compatible API)
    pub async fn check_motion_limits(&self, axis: &str, position: f32) -> SafetyCheckResult {
        let config = self.config.read().unwrap();
        let checker = self.motion_checker.read().unwrap();
        let default_checker = MotionSafetyChecker::new(&config);
        let checker = checker.as_ref().unwrap_or(&default_checker);
        checker.check_axis(axis, position)
    }

    // ---- Run all checks ----

    /// Run all safety checks (motion + temperature + hardware)
    pub async fn run_all_checks(&self) -> Vec<SafetyCheckResult> {
        let mut results = Vec::new();

        // 1. Motion checks — get position first (before any non-Send guards)
        let position = self.device_state.get_position().await;

        let config = self.config.read().unwrap();
        {
            let checker = self.motion_checker.read().unwrap();
            let default_checker = MotionSafetyChecker::new(&config);
            let checker = checker.as_ref().unwrap_or(&default_checker);
            let axes = [("x", position.x), ("y", position.y), ("z", position.z)];
            results.extend(checker.check_all_axes(&axes));
        }
        drop(config);

        // 2. Temperature checks (requires HeaterReading data from outside)
        // Caller should use on_temperature_update() for this.

        // 3. Hardware checks (state staleness — future)

        results
    }

    /// Check if any safety violation exists
    pub async fn has_safety_violation(&self) -> bool {
        let results = self.run_all_checks().await;
        results.iter().any(|r| !r.passed)
    }

    // ---- Recovery ----

    /// Recover from error — clear emergency stop and reset state
    pub async fn recover_from_error(&self) -> EmbResult<()> {
        self.clear_emergency_stop().await?;

        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "safety".to_string(),
            "Safety recovery completed".to_string(),
        ).with_severity(EventSeverity::Info));

        log::info!("Safety recovery completed");
        Ok(())
    }

    // ---- Config access ----

    /// Get safety configuration (returns a clone for thread safety)
    pub fn config(&self) -> SafetyConfig {
        self.config.read().unwrap().clone()
    }

    /// Update safety configuration and reload all checkers
    pub fn update_config(&self, config: SafetyConfig) {
        *self.config.write().unwrap() = config.clone();

        // Reload sub-checkers
        if let Ok(mut mc) = self.motion_checker.write() {
            *mc = Some(MotionSafetyChecker::new(&config));
        }
        if let Ok(mut tc) = self.temperature_checker.write() {
            *tc = Some(TemperatureSafetyChecker::new(&config));
        }
        if let Ok(mut hc) = self.hardware_checker.write() {
            *hc = Some(HardwareSafetyChecker::new(
                config.enable_safety_checks,
                config.state_stale_threshold_ms,
            ));
        }
    }

    // ---- Helper ----

    /// Publish a safety alert event
    #[allow(dead_code)]
    fn publish_safety_alert(&self, message: &str, severity: EventSeverity) {
        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "safety".to_string(),
            message.to_string(),
        ).with_severity(severity));
    }
}

impl Default for SafetyController {
    fn default() -> Self {
        Self::new(
            SafetyConfig::default(),
            Arc::new(DeviceStateManager::new(
                Arc::new(crate::core_client::CoreSocketClient::new(
                    crate::core_client::CoreClientConfig::default(),
                )),
                Arc::new(crate::common::SyncEventPublisher::new()),
                DeviceStateConfig::default(),
            )),
            Arc::new(crate::common::SyncEventPublisher::new()),
        )
    }
}