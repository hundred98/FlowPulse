//! Safety controller module
//!
//! This module provides safety checks and emergency stop handling,
//! ensuring the printer operates within safe limits.

use crate::common::{EmbResult, EventPublisher, EventKind, EventSeverity, PrinterEvent};
use crate::state::{DeviceStateManager, DeviceStateConfig};
use std::sync::Arc;
use std::collections::HashMap;

/// Safety configuration
#[derive(Debug, Clone)]
pub struct SafetyConfig {
    /// Motion limits for axes
    pub motion_limits: HashMap<String, MotionLimit>,

    /// Emergency stop timeout (milliseconds)
    pub emergency_stop_timeout_ms: u64,

    /// Enable safety checks
    pub enable_safety_checks: bool,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        let mut motion_limits = HashMap::new();
        motion_limits.insert("x".to_string(), MotionLimit { min: 0.0, max: 300.0 });
        motion_limits.insert("y".to_string(), MotionLimit { min: 0.0, max: 300.0 });
        motion_limits.insert("z".to_string(), MotionLimit { min: 0.0, max: 400.0 });

        Self {
            motion_limits,
            emergency_stop_timeout_ms: 1000,
            enable_safety_checks: true,
        }
    }
}

/// Temperature limit configuration
#[derive(Debug, Clone, Copy)]
pub struct TemperatureLimit {
    /// Minimum temperature
    pub min: f32,

    /// Maximum temperature
    pub max: f32,
}

/// Motion limit configuration
#[derive(Debug, Clone, Copy)]
pub struct MotionLimit {
    /// Minimum position
    pub min: f32,
    
    /// Maximum position
    pub max: f32,
}

/// Safety check result
#[derive(Debug, Clone)]
pub struct SafetyCheckResult {
    /// Check passed
    pub passed: bool,
    
    /// Check name
    pub check_name: String,
    
    /// Warning message (if check failed)
    pub message: Option<String>,
    
    /// Severity level
    pub severity: EventSeverity,
}

impl SafetyCheckResult {
    /// Create a passed result
    pub fn passed(check_name: String) -> Self {
        Self {
            passed: true,
            check_name,
            message: None,
            severity: EventSeverity::Info,
        }
    }
    
    /// Create a failed result
    pub fn failed(check_name: String, message: String, severity: EventSeverity) -> Self {
        Self {
            passed: false,
            check_name,
            message: Some(message),
            severity,
        }
    }
}

/// Safety controller
/// Manages safety checks and emergency stop handling
pub struct SafetyController {
    /// Safety configuration
    config: SafetyConfig,
    
    /// Device state manager
    device_state: Arc<DeviceStateManager>,
    
    /// Event publisher
    event_publisher: Arc<dyn EventPublisher>,
    
    /// Emergency stop flag
    emergency_stop_active: Arc<tokio::sync::RwLock<bool>>,
}

impl SafetyController {
    /// Create a new safety controller
    pub fn new(
        config: SafetyConfig,
        device_state: Arc<DeviceStateManager>,
        event_publisher: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            config,
            device_state,
            event_publisher,
            emergency_stop_active: Arc::new(tokio::sync::RwLock::new(false)),
        }
    }

    /// Check motion limits
    pub async fn check_motion_limits(&self, axis: &str, position: f32) -> SafetyCheckResult {
        if !self.config.enable_safety_checks {
            return SafetyCheckResult::passed("motion_limits".to_string());
        }
        
        let limit = self.config.motion_limits.get(axis);
        if limit.is_none() {
            return SafetyCheckResult::passed("motion_limits".to_string());
        }
        
        let limit = limit.unwrap();
        if position < limit.min {
            let message = format!("Position {} too low: {} < {}", axis, position, limit.min);
            self.publish_safety_alert(&message, EventSeverity::Warning);
            return SafetyCheckResult::failed("motion_limits".to_string(), message, EventSeverity::Warning);
        }
        
        if position > limit.max {
            let message = format!("Position {} too high: {} > {}", axis, position, limit.max);
            self.publish_safety_alert(&message, EventSeverity::Critical);
            return SafetyCheckResult::failed("motion_limits".to_string(), message, EventSeverity::Critical);
        }
        
        SafetyCheckResult::passed("motion_limits".to_string())
    }
    
    /// Handle emergency stop
    pub async fn handle_emergency_stop(&self) -> EmbResult<()> {
        // Set emergency stop flag
        let mut emergency_stop = self.emergency_stop_active.write().await;
        *emergency_stop = true;
        
        // Publish emergency stop event
        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "safety".to_string(),
            "Emergency stop activated".to_string(),
        ).with_severity(EventSeverity::Critical));
        
        // TODO: Send emergency stop command to core server
        // self.client.send_emergency_stop()?;
        
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
        
        // Publish emergency stop cleared event
        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "safety".to_string(),
            "Emergency stop cleared".to_string(),
        ).with_severity(EventSeverity::Info));
        
        log::info!("Emergency stop cleared");
        Ok(())
    }
    
    /// Run all safety checks
    pub async fn run_all_checks(&self) -> Vec<SafetyCheckResult> {
        let mut results = Vec::new();

        // Check positions
        let position = self.device_state.get_position().await;
        let axes = [("x", position.x), ("y", position.y), ("z", position.z)];
        for (axis, pos) in axes.iter() {
            let result = self.check_motion_limits(axis, *pos).await;
            results.push(result);
        }

        results
    }
    
    /// Check if any safety check failed
    pub async fn has_safety_violation(&self) -> bool {
        let results = self.run_all_checks().await;
        results.iter().any(|r| !r.passed)
    }
    
    /// Recover from error
    pub async fn recover_from_error(&self) -> EmbResult<()> {
        // Clear emergency stop
        self.clear_emergency_stop().await?;
        
        // Reset device state
        // TODO: Implement actual recovery logic
        
        // Publish recovery event
        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "safety".to_string(),
            "Safety recovery completed".to_string(),
        ).with_severity(EventSeverity::Info));
        
        log::info!("Safety recovery completed");
        Ok(())
    }
    
    /// Publish safety alert
    fn publish_safety_alert(&self, message: &str, severity: EventSeverity) {
        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "safety".to_string(),
            message.to_string(),
        ).with_severity(severity));
    }
    
    /// Get safety configuration
    pub fn config(&self) -> &SafetyConfig {
        &self.config
    }
    
    /// Update safety configuration
    pub fn update_config(&mut self, config: SafetyConfig) {
        self.config = config;
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