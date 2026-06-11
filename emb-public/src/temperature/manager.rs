//! Temperature manager
//!
//! This module provides the main temperature management functionality,
//! including temperature state management, safety checks, preset management,
//! and PID auto-tuning.

use super::preset::PresetManager;
use super::safety::TemperatureSafetyChecker;
use super::types::{
    HeaterState, SafetyAction, SafetyCheckResult, TemperatureManagerConfig, TemperaturePreset,
};
use super::pid_tune::{PidTuneProtocol, PidTuneResult, TuneProgress, PidTuneSubType};
use crate::common::{
    EmbError, EmbResult, EventPublisher, PrinterEvent, EventKind, EventSeverity,
    TempStatus,
};
use crate::config::{ConfigFrameBuilder, ConfigManager, TemperatureSafetyConfig};
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

    /// PID tune state
    tune_state: Arc<RwLock<TuneState>>,

    /// Configuration
    config: TemperatureManagerConfig,
}

/// PID tune state (for tracking ongoing tune process)
#[derive(Debug, Clone, Default)]
struct TuneState {
    /// Whether tuning is in progress
    in_progress: bool,
    
    /// Heater being tuned
    heater: Option<String>,
    
    /// Heater ID
    heater_id: Option<u8>,
    
    /// Current progress
    progress: Option<TuneProgress>,
    
    /// Tuning result
    result: Option<PidTuneResult>,
}

impl TemperatureManager {
    /// Create a new temperature manager
    pub fn new(
        client: Arc<CoreSocketClient>,
        event_publisher: Arc<dyn EventPublisher>,
        config: TemperatureManagerConfig,
        safety_config: Option<TemperatureSafetyConfig>,
    ) -> Self {
        let safety_config = safety_config.unwrap_or_default();
        Self {
            heaters: Arc::new(RwLock::new(HashMap::new())),
            client,
            event_publisher,
            safety_checker: TemperatureSafetyChecker::new(safety_config),
            preset_manager: PresetManager::new(),
            tune_state: Arc::new(RwLock::new(TuneState::default())),
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

        // Get safety configuration if available
        let safety_config = config.temperature_safety.as_ref();

        // Add bed heater (heater_id = 0)
        let bed_heater = if let Some(safety_cfg) = safety_config {
            if let Some(bed_safety) = safety_cfg.heaters.get("bed") {
                HeaterState::with_sensor_fault_thresholds(
                    "bed".to_string(),
                    0,
                    config.temperature.hotbed.min_temp as f32,
                    config.temperature.hotbed.max_temp as f32,
                    bed_safety.sensor_fault.max_temp,
                    bed_safety.sensor_fault.min_temp,
                )
            } else {
                HeaterState::new(
                    "bed".to_string(),
                    0,
                    config.temperature.hotbed.min_temp as f32,
                    config.temperature.hotbed.max_temp as f32,
                )
            }
        } else {
            HeaterState::new(
                "bed".to_string(),
                0,
                config.temperature.hotbed.min_temp as f32,
                config.temperature.hotbed.max_temp as f32,
            )
        };
        heaters.insert("bed".to_string(), bed_heater);

        // Add hotend heater (heater_id = 1)
        let hotend_heater = if let Some(safety_cfg) = safety_config {
            if let Some(hotend_safety) = safety_cfg.heaters.get("hotend") {
                HeaterState::with_sensor_fault_thresholds(
                    "hotend".to_string(),
                    1,
                    config.temperature.hotend.min_temp as f32,
                    config.temperature.hotend.max_temp as f32,
                    hotend_safety.sensor_fault.max_temp,
                    hotend_safety.sensor_fault.min_temp,
                )
            } else {
                HeaterState::new(
                    "hotend".to_string(),
                    1,
                    config.temperature.hotend.min_temp as f32,
                    config.temperature.hotend.max_temp as f32,
                )
            }
        } else {
            HeaterState::new(
                "hotend".to_string(),
                1,
                config.temperature.hotend.min_temp as f32,
                config.temperature.hotend.max_temp as f32,
            )
        };
        heaters.insert("hotend".to_string(), hotend_heater);

        // TODO: Support additional heaters from config

        log::info!("Loaded {} heaters from config", heaters.len());
        drop(heaters);

        // Load temperature presets
        self.load_presets_from_config(&config).await?;

        Ok(())
    }

    /// Load temperature presets from configuration
    async fn load_presets_from_config(&self, config: &crate::config::PrinterJsonConfig) -> EmbResult<()> {
        // Clear existing presets
        self.preset_manager.clear().await;

        // Add presets from config
        for preset_config in &config.temperature_presets {
            let preset = TemperaturePreset {
                name: preset_config.name.clone(),
                hotend_temp: preset_config.hotend_temp,
                bed_temp: preset_config.bed_temp,
                chamber_temp: preset_config.chamber_temp,
                fan_speed: preset_config.fan_speed,
            };

            self.preset_manager.add(preset).await?;
        }

        log::info!("Loaded {} temperature presets", config.temperature_presets.len());
        Ok(())
    }

    /// Subscribe to temperature updates from the device
    ///
    /// This method sets up a callback to receive status reports from the device
    /// and automatically update the current temperature values.
    pub async fn subscribe_temperature_updates(&self) -> EmbResult<()> {
        let heaters = self.heaters.clone();
        let event_publisher = self.event_publisher.clone();
        
        // Clone for tune frame handling
        let tune_state = self.tune_state.clone();

        // Set status report callback
        self.client.set_status_report_callback(move |frame_type, payload| {
            // Handle DeviceStatusReport (frame_type = 0x04)
            if frame_type == 0x04 && payload.len() >= 25 {
                // Parse temperature data (in 0.1°C units)
                let temp_bed_cur = i16::from_be_bytes([payload[17], payload[18]]) as f32 / 10.0;
                let temp_bed_tgt = i16::from_be_bytes([payload[19], payload[20]]) as f32 / 10.0;
                let temp_nozzle_cur = i16::from_be_bytes([payload[21], payload[22]]) as f32 / 10.0;
                let temp_nozzle_tgt = i16::from_be_bytes([payload[23], payload[24]]) as f32 / 10.0;

                // Update temperature in async context
                let heaters_clone = heaters.clone();
                let event_publisher_clone = event_publisher.clone();

                tokio::spawn(async move {
                    // Update bed temperature
                    let mut heaters = heaters_clone.write().await;
                    if let Some(bed_state) = heaters.get_mut("bed") {
                        bed_state.update_current(temp_bed_cur);
                        bed_state.set_target(temp_bed_tgt);
                    }

                    // Update hotend temperature
                    if let Some(hotend_state) = heaters.get_mut("hotend") {
                        hotend_state.update_current(temp_nozzle_cur);
                        hotend_state.set_target(temp_nozzle_tgt);
                    }
                    drop(heaters);

                    // Publish temperature update event
                    let _ = event_publisher_clone.publish(
                        PrinterEvent::new(
                            EventKind::TemperatureUpdate,
                            "temperature".to_string(),
                            format!("Temperature updated: bed={}/{}°C, hotend={}/{}°C",
                                temp_bed_cur, temp_bed_tgt,
                                temp_nozzle_cur, temp_nozzle_tgt),
                        ).with_severity(EventSeverity::Info),
                    );
                });
            }
            
            // Handle TEMPERATURE frame (frame_type = 0x02) for PID tuning
            if frame_type == 0x02 && !payload.is_empty() {
                let sub_type = payload[0];
                
                // Handle PROGRESS frame (sub_type = 0x13)
                if sub_type == 0x13 && payload.len() >= 8 {
                    let heater_id = payload[1];
                    let phase = payload[2];
                    let current_cycle = payload[3];
                    let total_cycles = payload[4];
                    
                    // Current temp (big-endian u16, value * 10)
                    let temp_raw = ((payload[5] as u16) << 8) | (payload[6] as u16);
                    let current_temp = temp_raw as f32 / 10.0;
                    
                    // Output power (value * 400)
                    let output_power = payload[7] as f32 / 400.0;
                    
                    // Debug: 输出原始字节
                    log::info!("📊 PID Raw: bytes={:?}, temp_raw={}, temp={:.1}, power={:.2}", 
                        &payload[..8], temp_raw, current_temp, output_power);
                    
                    let tune_state_clone = tune_state.clone();
                    let event_clone = event_publisher.clone();
                    
                    tokio::spawn(async move {
                        use super::pid_tune::{TuneProgress, TunePhase};
                        
                        let progress = TuneProgress {
                            heater_id,
                            phase: TunePhase::from(phase),
                            current_cycle,
                            total_cycles,
                            current_temp,
                            output_power,
                        };
                        
                        // Update tune state
                        {
                            let mut state = tune_state_clone.write().await;
                            state.progress = Some(progress.clone());
                        }
                        
                        // Publish progress event
                        let _ = event_clone.publish(
                            PrinterEvent::new(
                                EventKind::Info,
                                "pid_tune".to_string(),
                                format!("PID tune: cycle {}/{}, temp={}°C", 
                                    current_cycle, total_cycles, current_temp),
                            ).with_severity(EventSeverity::Info),
                        );
                    });
                }
                
                // Handle COMPLETE frame (sub_type = 0x14)
                if sub_type == 0x14 && payload.len() >= 29 {
                    let success = payload.get(2).unwrap_or(&0);
                    let error_code = payload.get(28).unwrap_or(&0);
                    let cycles_done = payload.get(15).unwrap_or(&0);
                    log::info!("🏁 PID Tune COMPLETE: heater={}, success={}, cycles={}, error_code={}, len={}", 
                        payload.get(1).unwrap_or(&0), 
                        success, 
                        cycles_done,
                        error_code,
                        payload.len());
                    
                    let tune_state_clone = tune_state.clone();
                    let event_clone = event_publisher.clone();
                    let payload_vec = payload.to_vec();
                    
                    tokio::spawn(async move {
                        use super::pid_tune::PidTuneProtocol;
                        
                        log::info!("🏁 Spawned: parsing COMPLETE...");
                        
                        let heater_name = {
                            let state = tune_state_clone.read().await;
                            state.heater.clone().unwrap_or_else(|| "unknown".to_string())
                        };
                        
                        match PidTuneProtocol::parse_complete(&payload_vec, &heater_name) {
                            Ok(result) => {
                                log::info!("🏁 Parse OK: success={}, in_progress set to false", result.success);
                                
                                // Update state
                                {
                                    let mut state = tune_state_clone.write().await;
                                    state.in_progress = false;
                                    state.result = Some(result.clone());
                                }
                                
                                if result.success {
                                    let _ = event_clone.publish(
                                        PrinterEvent::new(
                                            EventKind::Info,
                                            "pid_tune".to_string(),
                                            format!("PID tuning complete: Kp={:.3}, Ki={:.3}, Kd={:.3}",
                                                result.new_pid.kp, result.new_pid.ki, result.new_pid.kd),
                                        ).with_severity(EventSeverity::Info),
                                    );
                                } else {
                                    let _ = event_clone.publish(
                                        PrinterEvent::new(
                                            EventKind::Error,
                                            "pid_tune".to_string(),
                                            format!("PID tuning failed: error code {}", result.error_code),
                                        ).with_severity(EventSeverity::Error),
                                    );
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to parse PID tune complete frame: {}", e);
                            }
                        }
                    });
                }
            }
        }).await;

        // Subscribe to status reports (notify server)
        self.client.subscribe_status(true).await?;

        log::info!("Subscribed to temperature updates from device");
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

        // Debug: log frame content
        log::info!("Temperature set frame: {:02X?}", frame);
        log::info!("Sending temperature frame: heater_id={}, temp={}°C, frame_len={} bytes",
            heater_id, temp, frame.len());

        // Send to device
        self.client.serial_send_raw(&frame).await
            .map_err(|e| {
                log::error!("Failed to send temperature frame: {}", e);
                EmbError::Communication(e)
            })?;

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
        self.preset_manager.add(preset).await?;

        // Save to configuration file
        self.save_presets_to_config().await?;

        Ok(())
    }

    /// Remove a preset
    pub async fn remove_preset(&self, name: &str) -> EmbResult<()> {
        self.preset_manager.remove(name).await?;

        // Save to configuration file
        self.save_presets_to_config().await?;

        Ok(())
    }

    /// Save current presets to configuration file
    async fn save_presets_to_config(&self) -> EmbResult<()> {
        use crate::config::TemperaturePresetConfig;

        // Get current presets
        let presets = self.preset_manager.get_all().await;

        // Convert to config format
        let preset_configs: Vec<TemperaturePresetConfig> = presets
            .iter()
            .map(|p| TemperaturePresetConfig {
                name: p.name.clone(),
                hotend_temp: p.hotend_temp,
                bed_temp: p.bed_temp,
                chamber_temp: p.chamber_temp,
                fan_speed: p.fan_speed,
            })
            .collect();

        // Save to configuration
        ConfigManager::instance()
            .save_temperature_presets(&preset_configs)
            .map_err(|e| EmbError::Config(e))?;

        log::info!("Saved {} presets to configuration", presets.len());
        Ok(())
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

    /// Get the underlying CoreSocketClient
    pub fn client(&self) -> Arc<CoreSocketClient> {
        self.client.clone()
    }

    /// Import presets from a list
    ///
    /// This will clear all existing presets and load the provided presets.
    /// Use this when you want to replace all presets with a new set.
    pub async fn import_presets(&self, presets: Vec<TemperaturePreset>) {
        self.preset_manager.load_from_config(presets).await;
    }

    /// Export presets to configuration format
    pub async fn export_presets(&self) -> Vec<TemperaturePreset> {
        self.preset_manager.export_to_config().await
    }

    // ==================== PID Auto-Tune Methods ====================

    /// Start PID auto-tuning for a heater
    ///
    /// This sends a START command to the lower machine, which will perform
    /// the actual tuning process.
    ///
    /// # Arguments
    /// * `heater` - Heater name ("hotend" or "bed")
    /// * `target_temp` - Target temperature in °C
    /// * `cycles` - Number of tuning cycles (recommended: 6-8)
    pub async fn start_pid_tune(
        &self,
        heater: &str,
        target_temp: f32,
        cycles: u8,
    ) -> EmbResult<()> {
        // Check if already tuning
        {
            let state = self.tune_state.read().await;
            if state.in_progress {
                return Err(EmbError::InvalidParam("PID tuning already in progress".to_string()));
            }
        }
        
        // Get heater ID
        let heater_id = match heater {
            "bed" => 0,
            "hotend" => 1,
            _ => return Err(EmbError::InvalidParam(format!("Unknown heater: {}", heater))),
        };
        
        // Build START frame payload
        let payload = PidTuneProtocol::build_start_payload(heater_id, target_temp, cycles);
        
        // Send to lower machine
        // Note: The frame will be wrapped by the client with SOF/LEN/TYPE/CRC8/EOF
        self.client.send_temperature_tune_frame(&payload).await
            .map_err(|e| EmbError::Communication(e))?;
        
        // Update state
        {
            let mut state = self.tune_state.write().await;
            state.in_progress = true;
            state.heater = Some(heater.to_string());
            state.heater_id = Some(heater_id);
            state.progress = None;
            state.result = None;
        }
        
        // Publish event
        let _ = self.event_publisher.publish(
            PrinterEvent::new(
                EventKind::Info,
                "pid_tune".to_string(),
                format!("Started PID tuning for {} at {}°C, {} cycles", heater, target_temp, cycles),
            ).with_severity(EventSeverity::Info),
        );
        
        log::info!("Started PID tuning: heater={}, target={}°C, cycles={}", heater, target_temp, cycles);
        Ok(())
    }

    /// Get current PID tuning progress
    pub async fn get_tune_progress(&self) -> Option<TuneProgress> {
        let state = self.tune_state.read().await;
        state.progress.clone()
    }

    /// Check if PID tuning is in progress
    pub async fn is_tuning(&self) -> bool {
        let state = self.tune_state.read().await;
        state.in_progress
    }

    /// Cancel ongoing PID tuning
    pub async fn cancel_pid_tune(&self) -> EmbResult<()> {
        // Get heater ID
        let heater_id = {
            let state = self.tune_state.read().await;
            if !state.in_progress {
                return Ok(()); // Not tuning, nothing to cancel
            }
            state.heater_id.ok_or_else(|| EmbError::InvalidParam("No heater being tuned".to_string()))?
        };
        
        // Build CANCEL frame payload
        let payload = PidTuneProtocol::build_cancel_payload(heater_id);
        
        // Send to lower machine
        self.client.send_temperature_tune_frame(&payload).await
            .map_err(|e| EmbError::Communication(e))?;
        
        // Update state
        {
            let mut state = self.tune_state.write().await;
            state.in_progress = false;
            state.heater = None;
            state.heater_id = None;
        }
        
        log::info!("Cancelled PID tuning");
        Ok(())
    }

    /// Get the last tuning result
    pub async fn get_tune_result(&self) -> Option<PidTuneResult> {
        let state = self.tune_state.read().await;
        state.result.clone()
    }

    /// Apply PID tuning result to configuration
    ///
    /// This will:
    /// 1. Update the configuration file with new PID parameters
    /// 2. Send the new parameters to the lower machine
    pub async fn apply_tune_result(&self) -> EmbResult<()> {
        // Get result
        let (heater, heater_id, params) = {
            let state = self.tune_state.read().await;
            let result = state.result.as_ref()
                .ok_or_else(|| EmbError::InvalidParam("No tuning result available".to_string()))?;
            
            if !result.success {
                return Err(EmbError::InvalidParam("Cannot apply failed tuning result".to_string()));
            }
            
            (
                state.heater.clone().unwrap_or_default(),
                state.heater_id.unwrap_or(0),
                result.new_pid,
            )
        };
        
        // Update configuration file
        ConfigManager::instance().update_temperature_pid(
            &heater,
            params.kp,
            params.ki,
            params.kd,
        ).map_err(|e| EmbError::Config(e))?;
        
        // Build APPLY frame payload
        let payload = PidTuneProtocol::build_apply_payload(heater_id, &params);
        
        // Send to lower machine
        self.client.send_temperature_tune_frame(&payload).await
            .map_err(|e| EmbError::Communication(e))?;
        
        // Clear state
        {
            let mut state = self.tune_state.write().await;
            state.in_progress = false;
            state.heater = None;
            state.heater_id = None;
        }
        
        // Publish event
        let _ = self.event_publisher.publish(
            PrinterEvent::new(
                EventKind::Info,
                "pid_tune".to_string(),
                format!(
                    "Applied new PID parameters for {}: Kp={:.3}, Ki={:.3}, Kd={:.3}",
                    heater, params.kp, params.ki, params.kd
                ),
            ).with_severity(EventSeverity::Info),
        );
        
        log::info!(
            "Applied new PID parameters: heater={}, Kp={:.3}, Ki={:.3}, Kd={:.3}",
            heater, params.kp, params.ki, params.kd
        );
        
        Ok(())
    }

    /// Handle PID tune frame from lower machine
    ///
    /// This is called when a TEMPERATURE frame with sub_type 0x13-0x15 is received.
    pub async fn handle_tune_frame(&self, payload: &[u8]) -> EmbResult<()> {
        if payload.is_empty() {
            return Err(EmbError::Protocol("Empty tune frame payload".to_string()));
        }
        
        let sub_type = PidTuneSubType::from(payload[0]);
        
        match sub_type {
            PidTuneSubType::Progress => {
                let progress = PidTuneProtocol::parse_progress(payload)?;
                
                // Update state
                {
                    let mut state = self.tune_state.write().await;
                    state.progress = Some(progress.clone());
                }
                
                // Publish progress event
                let _ = self.event_publisher.publish(
                    PrinterEvent::new(
                        EventKind::Info,
                        "pid_tune".to_string(),
                        format!(
                            "PID tune progress: {}% (cycle {}/{})",
                            progress.percent(),
                            progress.current_cycle,
                            progress.total_cycles
                        ),
                    ).with_severity(EventSeverity::Info),
                );
            }
            
            PidTuneSubType::Complete => {
                let heater_name = {
                    let state = self.tune_state.read().await;
                    state.heater.clone().unwrap_or_else(|| "unknown".to_string())
                };
                
                let result = PidTuneProtocol::parse_complete(payload, &heater_name)?;
                
                // Update state
                {
                    let mut state = self.tune_state.write().await;
                    state.in_progress = false;
                    state.result = Some(result.clone());
                }
                
                if result.success {
                    let _ = self.event_publisher.publish(
                        PrinterEvent::new(
                            EventKind::Info,
                            "pid_tune".to_string(),
                            format!(
                                "PID tuning complete for {}: Kp={:.3}, Ki={:.3}, Kd={:.3}",
                                heater_name, result.new_pid.kp, result.new_pid.ki, result.new_pid.kd
                            ),
                        ).with_severity(EventSeverity::Info),
                    );
                    
                    log::info!(
                        "PID tuning complete: heater={}, Kp={:.3}, Ki={:.3}, Kd={:.3}",
                        heater_name, result.new_pid.kp, result.new_pid.ki, result.new_pid.kd
                    );
                } else {
                    let _ = self.event_publisher.publish(
                        PrinterEvent::new(
                            EventKind::Error,
                            "pid_tune".to_string(),
                            format!("PID tuning failed for {}: error code {}", heater_name, result.error_code),
                        ).with_severity(EventSeverity::Error),
                    );
                    
                    log::error!("PID tuning failed: heater={}, error code {}", heater_name, result.error_code);
                }
            }
            
            PidTuneSubType::Ack => {
                let (sub_type, success, error_code) = PidTuneProtocol::parse_ack(payload)?;
                
                if !success {
                    log::warn!("PID tune ACK: sub_type=0x{:02X}, error_code={}", sub_type, error_code);
                }
            }
            
            _ => {
                log::warn!("Unexpected PID tune sub_type: 0x{:02X}", payload[0]);
            }
        }
        
        Ok(())
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
