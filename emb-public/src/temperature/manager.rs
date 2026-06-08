//! Temperature manager
//!
//! This module provides the main temperature management functionality,
//! including temperature state management, safety checks, and preset management.

use super::preset::PresetManager;
use super::safety::TemperatureSafetyChecker;
use super::types::{
    HeaterState, SafetyAction, SafetyCheckResult, TemperatureManagerConfig, TemperaturePreset,
};
use crate::common::{
    EmbError, EmbResult, EventPublisher, PrinterEvent, EventKind, EventSeverity,
    TempStatus,
};
use crate::config::{ConfigFrameBuilder, ConfigManager};
use crate::core_client::CoreSocketClient;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Temperature manager
pub struct TemperatureManager {
    /// Heater states
    heaters: Arc<RwLock<HashMap<String, HeaterState>>>,

    /// Core client
    client: Arc<CoreSocketClient>,

    /// Event publisher
    event_publisher: Arc<dyn EventPublisher>,

    /// Safety checker
    safety_checker: TemperatureSafetyChecker,

    /// Preset manager
    preset_manager: PresetManager,

    /// Configuration
    config: TemperatureManagerConfig,
}

impl TemperatureManager {
    /// Create a new temperature manager
    pub fn new(
        client: Arc<CoreSocketClient>,
        event_publisher: Arc<dyn EventPublisher>,
        config: TemperatureManagerConfig,
    ) -> Self {
        Self {
            heaters: Arc::new(RwLock::new(HashMap::new())),
            client,
            event_publisher,
            safety_checker: TemperatureSafetyChecker::new(config.clone()),
            preset_manager: PresetManager::new(),
            config,
        }
    }

    /// Initialize from ConfigManager
    pub async fn initialize(&self) -> EmbResult<()> {
        // Load configuration
        self.load_config().await?;

        // Register config change callback
        // Note: This requires a way to notify TemperatureManager when config changes
        // For now, we'll handle this through the reload method

        log::info!("Temperature manager initialized");
        Ok(())
    }

    /// Load configuration from ConfigManager
    async fn load_config(&self) -> EmbResult<()> {
        let config = ConfigManager::instance().get_config()?;

        let mut heaters = self.heaters.write().await;
        heaters.clear();

        // Add bed heater (heater_id = 0)
        heaters.insert(
            "bed".to_string(),
            HeaterState::new(
                "bed".to_string(),
                0,
                config.temperature.hotbed.min_temp as f32,
                config.temperature.hotbed.max_temp as f32,
            ),
        );

        // Add hotend heater (heater_id = 1)
        heaters.insert(
            "hotend".to_string(),
            HeaterState::new(
                "hotend".to_string(),
                1,
                config.temperature.hotend.min_temp as f32,
                config.temperature.hotend.max_temp as f32,
            ),
        );

        // TODO: Support additional heaters from config

        log::info!("Loaded {} heaters from config", heaters.len());
        Ok(())
    }

    /// Set target temperature for a heater
    pub async fn set_target(&self, heater: &str, temp: f32) -> EmbResult<()> {
        // Get heater state
        let heaters = self.heaters.read().await;
        let heater_state = heaters
            .get(heater)
            .ok_or_else(|| EmbError::InvalidParam(format!("Unknown heater: {}", heater)))?;

        // Safety check
        if temp < heater_state.min_temp || temp > heater_state.max_temp {
            return Err(EmbError::InvalidParam(format!(
                "Temperature {} out of range [{}, {}]",
                temp, heater_state.min_temp, heater_state.max_temp
            )));
        }

        let heater_id = heater_state.heater_id;
        drop(heaters);

        // Update target temperature
        {
            let mut heaters = self.heaters.write().await;
            if let Some(state) = heaters.get_mut(heater) {
                state.set_target(temp);
            }
        }

        // Build config frame
        let frame = ConfigFrameBuilder::build_set_temp_frame(heater_id, temp);

        // Send to device
        self.client.serial_send_raw(&frame).await?;

        // Publish event
        let _ = self.event_publisher.publish(
            PrinterEvent::new(
                EventKind::TemperatureUpdate,
                "temperature".to_string(),
                format!("Set {} temperature to {}", heater, temp),
            )
            .with_severity(EventSeverity::Info),
        );

        log::info!("Set {} target temperature to {}°C", heater, temp);
        Ok(())
    }

    /// Set target temperatures for multiple heaters
    pub async fn set_targets(&self, targets: HashMap<String, f32>) -> EmbResult<()> {
        for (heater, temp) in targets {
            self.set_target(&heater, temp).await?;
        }
        Ok(())
    }

    /// Get heater state
    pub async fn get_heater(&self, heater: &str) -> Option<HeaterState> {
        let heaters = self.heaters.read().await;
        heaters.get(heater).cloned()
    }

    /// Get all heater states
    pub async fn get_all_heaters(&self) -> HashMap<String, HeaterState> {
        self.heaters.read().await.clone()
    }

    /// Update current temperature (called from status report)
    pub async fn update_current(&self, heater: &str, temp: f32) {
        let mut heaters = self.heaters.write().await;
        if let Some(state) = heaters.get_mut(heater) {
            let old_temp = state.current_temp;
            state.update_current(temp);

            // Check if temperature changed significantly
            if (temp - old_temp).abs() > self.config.temp_change_threshold {
                // Publish temperature update event
                let _ = self.event_publisher.publish(
                    PrinterEvent::new(
                        EventKind::TemperatureUpdate,
                        "temperature".to_string(),
                        format!("{} temperature: {:.1}°C", heater, temp),
                    )
                    .with_severity(EventSeverity::Info),
                );
            }
        }
    }

    /// Update current temperatures from status report
    pub async fn update_from_status_report(&self, bed_current: f32, hotend_current: f32) {
        self.update_current("bed", bed_current).await;
        self.update_current("hotend", hotend_current).await;

        // Perform safety check on update
        if self.config.enable_auto_safety_check {
            let results = self.check_safety().await;
            self.handle_safety_results(results).await;
        }
    }

    /// Apply temperature preset
    pub async fn apply_preset(&self, preset_name: &str) -> EmbResult<()> {
        let preset = self
            .preset_manager
            .get(preset_name)
            .await
            .ok_or_else(|| {
                EmbError::InvalidParam(format!("Preset '{}' not found", preset_name))
            })?;

        // Set temperatures
        let mut targets = HashMap::new();
        targets.insert("hotend".to_string(), preset.hotend_temp);
        targets.insert("bed".to_string(), preset.bed_temp);

        if let Some(chamber_temp) = preset.chamber_temp {
            // TODO: Support chamber heater when implemented
            log::info!("Chamber temperature: {}°C (not yet supported)", chamber_temp);
        }

        self.set_targets(targets).await?;

        log::info!("Applied preset: {}", preset_name);
        Ok(())
    }

    /// Get all presets
    pub async fn get_presets(&self) -> Vec<TemperaturePreset> {
        self.preset_manager.get_all().await
    }

    /// Add a preset
    pub async fn add_preset(&self, preset: TemperaturePreset) -> EmbResult<()> {
        self.preset_manager.add(preset).await
    }

    /// Remove a preset
    pub async fn remove_preset(&self, name: &str) -> EmbResult<()> {
        self.preset_manager.remove(name).await
    }

    /// Perform safety check
    pub async fn check_safety(&self) -> Vec<SafetyCheckResult> {
        let heaters = self.heaters.read().await;
        let heater_list: Vec<_> = heaters.values().cloned().collect();
        drop(heaters);

        self.safety_checker.check_heaters(&heater_list)
    }

    /// Handle safety check results
    async fn handle_safety_results(&self, results: Vec<SafetyCheckResult>) {
        for result in results {
            if result.needs_action() {
                log::warn!(
                    "⚠️  Safety check: {} - {}",
                    result.heater,
                    result.message
                );

                self.execute_safety_action(&result).await;
            }
        }
    }

    /// Execute safety action
    async fn execute_safety_action(&self, result: &SafetyCheckResult) {
        match result.action {
            SafetyAction::None => {}
            SafetyAction::Warn => {
                let _ = self.event_publisher.publish(
                    PrinterEvent::new(
                        EventKind::SafetyWarning,
                        "temperature".to_string(),
                        result.message.clone(),
                    )
                    .with_severity(EventSeverity::Warning),
                );
            }
            SafetyAction::TurnOffHeater => {
                log::warn!("Turning off heater: {}", result.heater);
                let _ = self.set_target(&result.heater, 0.0).await;
            }
            SafetyAction::PausePrint => {
                log::error!("Critical temperature issue, pausing print: {}", result.message);
                // TODO: Call PrintController::pause()
                let _ = self.event_publisher.publish(
                    PrinterEvent::new(
                        EventKind::SafetyWarning,
                        "temperature".to_string(),
                        result.message.clone(),
                    )
                    .with_severity(EventSeverity::Error),
                );
            }
            SafetyAction::EmergencyStop => {
                log::error!("🚨 Emergency stop triggered: {}", result.message);
                // TODO: Call EmergencyStop
                let _ = self.event_publisher.publish(
                    PrinterEvent::new(
                        EventKind::SafetyWarning,
                        "temperature".to_string(),
                        result.message.clone(),
                    )
                    .with_severity(EventSeverity::Critical),
                );
            }
        }
    }

    /// Start periodic safety check loop
    pub async fn start_safety_check_loop(&self) {
        let interval = Duration::from_millis(self.config.safety_check_interval_ms);

        loop {
            // Perform safety check
            let results = self.check_safety().await;

            // Handle results
            self.handle_safety_results(results).await;

            // Wait for next check
            tokio::time::sleep(interval).await;
        }
    }

    /// Turn off all heaters
    pub async fn turn_off_all(&self) -> EmbResult<()> {
        let heaters = self.heaters.read().await;
        let heater_names: Vec<_> = heaters.keys().cloned().collect();
        drop(heaters);

        for heater in heater_names {
            self.set_target(&heater, 0.0).await?;
        }

        log::info!("All heaters turned off");
        Ok(())
    }

    /// Get temperature status (for FrontendDataProvider)
    pub async fn get_temp_status(&self) -> TempStatus {
        let heaters = self.heaters.read().await;

        let hotend_current = heaters
            .get("hotend")
            .map(|h| h.current_temp)
            .unwrap_or(0.0);
        let hotend_target = heaters
            .get("hotend")
            .map(|h| h.target_temp)
            .unwrap_or(0.0);
        let bed_current = heaters.get("bed").map(|h| h.current_temp).unwrap_or(0.0);
        let bed_target = heaters.get("bed").map(|h| h.target_temp).unwrap_or(0.0);

        TempStatus::new(hotend_current, hotend_target, bed_current, bed_target)
    }

    /// Load presets from configuration
    pub async fn load_presets(&self, presets: Vec<TemperaturePreset>) {
        self.preset_manager.load_from_config(presets).await;
    }

    /// Export presets to configuration format
    pub async fn export_presets(&self) -> Vec<TemperaturePreset> {
        self.preset_manager.export_to_config().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::SyncEventPublisher;

    #[tokio::test]
    async fn test_temperature_manager_initialization() {
        // This test requires ConfigManager to be initialized
        // In a real test, we would mock the dependencies
    }
}
