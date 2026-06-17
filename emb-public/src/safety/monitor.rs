//! Safety monitor — periodic safety check loop
//!
//! This module provides the `SafetyMonitor`, which runs periodic safety checks
//! at a configurable interval, invoking all sub-checkers (motion, temperature,
//! hardware) and dispatching the resulting safety actions.

use super::actions::SafetyActionExecutor;
use super::hardware::HardwareSafetyChecker;
use super::motion::MotionSafetyChecker;
use super::temperature::{HeaterReading, TemperatureSafetyChecker};
use super::types::SafetyCheckResult;
use std::sync::Arc;

/// Data source trait — allows the monitor to query current device state
/// without depending on concrete types.
pub trait SafetyDataSource: Send + Sync {
    /// Get current axis positions as [(axis_name, position), ...]
    fn get_positions(&self) -> Vec<(&str, f32)>;

    /// Get heater readings
    fn get_heater_readings(&self) -> Vec<HeaterReading>;

    /// Get elapsed time since last device state update (milliseconds)
    fn state_elapsed_ms(&self) -> u64;
}

/// Periodically runs safety checks and dispatches actions.
pub struct SafetyMonitor {
    /// Motion checker
    motion: std::sync::RwLock<MotionSafetyChecker>,

    /// Temperature checker
    temperature: std::sync::RwLock<TemperatureSafetyChecker>,

    /// Hardware checker
    hardware: std::sync::RwLock<HardwareSafetyChecker>,

    /// Action executor
    action_executor: SafetyActionExecutor,

    /// Data source for reading current state
    data_source: Arc<dyn SafetyDataSource>,

    /// Check interval
    interval_ms: u64,

    /// Shutdown signal
    shutdown: Option<tokio::sync::watch::Receiver<bool>>,
}

impl SafetyMonitor {
    /// Create a new safety monitor
    pub fn new(
        motion: MotionSafetyChecker,
        temperature: TemperatureSafetyChecker,
        hardware: HardwareSafetyChecker,
        action_executor: SafetyActionExecutor,
        data_source: Arc<dyn SafetyDataSource>,
        interval_ms: u64,
    ) -> Self {
        Self {
            motion: std::sync::RwLock::new(motion),
            temperature: std::sync::RwLock::new(temperature),
            hardware: std::sync::RwLock::new(hardware),
            action_executor,
            data_source,
            interval_ms,
            shutdown: None,
        }
    }

    /// Set a shutdown receiver to stop the monitor loop
    pub fn with_shutdown(mut self, shutdown: tokio::sync::watch::Receiver<bool>) -> Self {
        self.shutdown = Some(shutdown);
        self
    }

    /// Run a single round of all safety checks
    pub async fn run_checks(&self) -> Vec<SafetyCheckResult> {
        let mut all_results = Vec::new();

        // 1. Motion checks
        {
            let checker = self.motion.read().unwrap();
            let positions = self.data_source.get_positions();
            let results = checker.check_all_axes(&positions);
            all_results.extend(results);
        }

        // 2. Temperature checks
        {
            let checker = self.temperature.read().unwrap();
            let readings = self.data_source.get_heater_readings();
            let results = checker.check_heaters(&readings);
            all_results.extend(results);
        }

        // 3. Hardware checks (state staleness)
        {
            let checker = self.hardware.read().unwrap();
            let elapsed = self.data_source.state_elapsed_ms();
            if let Some(result) = checker.check_state_staleness(elapsed) {
                all_results.push(result);
            }
        }

        all_results
    }

    /// Run checks and dispatch actions
    pub async fn run_and_act(&self) {
        let results = self.run_checks().await;
        self.action_executor.execute_all(&results).await;
    }

    /// Start the main safety check loop.
    /// This will run forever unless a shutdown signal is configured.
    pub async fn run_loop(mut self) {
        let interval = tokio::time::Duration::from_millis(self.interval_ms);

        log::info!(
            "Safety monitor started (interval={}ms)",
            self.interval_ms
        );

        // Take ownership of shutdown receiver to avoid borrow conflicts with self
        let mut shutdown_rx = self.shutdown.take();
        if let Some(ref mut shutdown) = shutdown_rx {
            loop {
                tokio::select! {
                    _ = shutdown.changed() => {
                        log::info!("Safety monitor shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(interval) => {
                        self.run_and_act().await;
                    }
                }
            }
        } else {
            loop {
                tokio::time::sleep(interval).await;
                self.run_and_act().await;
            }
        }
    }

    // ---- Hot-reload support ----

    /// Reload motion checker config
    pub fn reload_motion(&self, checker: MotionSafetyChecker) {
        *self.motion.write().unwrap() = checker;
    }

    /// Reload temperature checker config
    pub fn reload_temperature(&self, checker: TemperatureSafetyChecker) {
        *self.temperature.write().unwrap() = checker;
    }

    /// Reload hardware checker config
    pub fn reload_hardware(&self, checker: HardwareSafetyChecker) {
        *self.hardware.write().unwrap() = checker;
    }

    /// Update check interval
    pub fn set_interval(&mut self, interval_ms: u64) {
        self.interval_ms = interval_ms;
    }
}