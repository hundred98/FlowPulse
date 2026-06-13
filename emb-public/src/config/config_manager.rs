//! Configuration Manager
//!
//! Centralized configuration management module. All configuration file reads
//! must go through this module, and other modules pull configuration from here.
//!
//! # Usage
//! ```ignore
//! use emb_public::config::ConfigManager;
//!
//! // Startup: load configuration
//! ConfigManager::instance().load("./config")?;
//!
//! // Get configuration
//! let config = ConfigManager::instance().get_config()?;
//!
//! // Register change callback
//! ConfigManager::instance().on_config_change(Box::new(|config| {
//!     log::info!("Config changed: {}", config.printer_model);
//! }));
//!
//! // Reload configuration (user triggered)
//! ConfigManager::instance().reload(&client).await?;
//! ```

use std::sync::{RwLock, Arc};
use once_cell::sync::Lazy;

use super::printer_config::PrinterJsonConfig;
use super::config_adapter::{load_configs, build_printer_config, build_motion_config_json, LoadedConfigs, BedMeshHardwareConfig};
use super::config_protocol::{ConfigFrameBuilder, validate_config};
use super::config_interlock::validate_bed_mesh_interlock;
use crate::CoreSocketClient;
use emb_api::{CoreRequest, CoreResponse, MotionRequest, MotionResponse, NackReason};

/// Configuration change callback type.
/// 
/// Called when configuration is loaded or reloaded.
pub type ConfigChangeCallback = Box<dyn Fn(&PrinterJsonConfig) + Send + Sync>;

/// Global configuration manager singleton.
/// 
/// All configuration reads must go through this instance.
pub static CONFIG_MANAGER: Lazy<ConfigManager> = Lazy::new(|| ConfigManager {
    inner: RwLock::new(ConfigInner::default()),
});

/// Internal configuration state
struct ConfigInner {
    /// Loaded printer configuration
    printer_config: Option<PrinterJsonConfig>,
    /// Raw loaded configs (for building motion config)
    loaded_configs: Option<LoadedConfigs>,
    /// Configuration directory path
    config_dir: String,
    /// Configuration change callbacks
    callbacks: Vec<Arc<ConfigChangeCallback>>,
}

impl Default for ConfigInner {
    fn default() -> Self {
        Self {
            printer_config: None,
            loaded_configs: None,
            config_dir: String::new(),
            callbacks: Vec::new(),
        }
    }
}

/// Configuration manager for centralized config access.
pub struct ConfigManager {
    inner: RwLock<ConfigInner>,
}

impl ConfigManager {
    /// Get the global ConfigManager instance.
    pub fn instance() -> &'static ConfigManager {
        &CONFIG_MANAGER
    }

    /// Register a callback to be called when configuration changes.
    /// 
    /// The callback will be called:
    /// - After `load()` completes successfully
    /// - After `reload()` completes successfully
    /// 
    /// # Arguments
    /// * `callback` - Function to call with the new configuration
    /// 
    /// # Example
    /// ```ignore
    /// ConfigManager::instance().on_config_change(Box::new(|config| {
    ///     log::info!("Temperature PID updated: kp={}", config.temperature.hotend.kp);
    ///     // Update local cache or reinitialize
    /// }));
    /// ```
    pub fn on_config_change(&self, callback: ConfigChangeCallback) {
        let mut inner = match self.inner.write() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("Failed to acquire lock for callback registration: {}", e);
                return;
            }
        };
        inner.callbacks.push(Arc::new(callback));
    }

    /// Clear all registered callbacks.
    pub fn clear_callbacks(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.callbacks.clear();
        }
    }

    /// Load configuration files at startup.
    /// 
    /// This reads `hardware.json`, `motion.json`, and `printer.json` from the
    /// specified directory and stores them in memory.
    /// 
    /// # Arguments
    /// * `config_dir` - Path to the configuration directory
    /// 
    /// # Returns
    /// * `Ok(())` if configuration was loaded successfully
    /// * `Err(String)` if any error occurred
    pub fn load(&self, config_dir: &str) -> Result<(), String> {
        log::info!("📁 Loading configuration from: {}", config_dir);
        
        let configs = load_configs(config_dir)?;
        
        // Validate bed mesh interlock rules before building printer config
        if let Some(mut bed_mesh) = configs.hardware.bed_mesh.clone() {
            validate_bed_mesh_interlock(&mut bed_mesh)?;
            // Update configs with corrected bed_mesh
            let mut configs_corrected = configs.clone();
            configs_corrected.hardware.bed_mesh = Some(bed_mesh);
            let printer_config = build_printer_config(&configs_corrected);
            
            // Validate configuration before using it
            validate_config(&printer_config)?;
            
            // Update cached config and get callbacks
            let callbacks = {
                let mut inner = self.inner.write().map_err(|e| format!("Lock error: {}", e))?;
                inner.config_dir = config_dir.to_string();
                inner.printer_config = Some(printer_config.clone());
                inner.loaded_configs = Some(configs_corrected);
                inner.callbacks.clone()
            };
            
            // Notify all registered callbacks
            Self::notify_callbacks(&callbacks, &printer_config);
        } else {
            let printer_config = build_printer_config(&configs);
            
            // Validate configuration before using it
            validate_config(&printer_config)?;
            
            // Update cached config and get callbacks
            let callbacks = {
                let mut inner = self.inner.write().map_err(|e| format!("Lock error: {}", e))?;
                inner.config_dir = config_dir.to_string();
                inner.printer_config = Some(printer_config.clone());
                inner.loaded_configs = Some(configs);
                inner.callbacks.clone()
            };
            
            // Notify all registered callbacks
            Self::notify_callbacks(&callbacks, &printer_config);
        }
        
        log::info!("✅ Configuration loaded successfully");
        Ok(())
    }

    /// Reload configuration and notify downstream systems.
    /// 
    /// This performs:
    /// 1. Re-read configuration files
    /// 2. Send motion config to server
    /// 3. Send hardware config frames to device (STM32)
    /// 4. Send ConfigComplete to device
    /// 5. Notify all registered callbacks
    /// 
    /// # Arguments
    /// * `client` - The CoreSocketClient for communication
    /// 
    /// # Returns
    /// * `Ok(())` if reload was successful
    /// * `Err(String)` if any error occurred
    pub async fn reload(&self, client: &CoreSocketClient) -> Result<(), String> {
        log::info!("🔄 Reloading configuration...");
        
        let config_dir = {
            let inner = self.inner.read().map_err(|e| format!("Lock error: {}", e))?;
            inner.config_dir.clone()
        };
        
        if config_dir.is_empty() {
            return Err("Configuration not loaded. Call load() first.".to_string());
        }

        // Step 1: Re-read configuration files
        let configs = load_configs(&config_dir)?;
        
        // Validate bed mesh interlock rules before building printer config
        let configs_corrected = if let Some(mut bed_mesh) = configs.hardware.bed_mesh.clone() {
            validate_bed_mesh_interlock(&mut bed_mesh)?;
            let mut configs_corrected = configs.clone();
            configs_corrected.hardware.bed_mesh = Some(bed_mesh);
            configs_corrected
        } else {
            configs.clone()
        };
        
        let printer_config = build_printer_config(&configs_corrected);
        
        // Step 1.5: Validate configuration before using it
        validate_config(&printer_config)?;
        
        // Step 2: Send motion config to server
        log::info!("📤 Sending motion config to server...");
        let motion_config_json = build_motion_config_json(&configs_corrected)?;
        client.config_update_motion(&motion_config_json).await
            .map_err(|e| format!("Failed to send motion config to server: {}", e))?;
        
        // Step 2.5: Send fan config to server
        log::info!("📤 Sending fan config to server...");
        let fan_config = emb_api::FanConfig {
            fans: configs_corrected.hardware.fan.as_ref()
                .map(|fan_list| {
                    fan_list.iter()
                        .map(|f| emb_api::FanConfigEntry {
                            index: f.index,
                            name: f.name.clone(),
                            description: f.description.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        client.config_update_fan(&fan_config).await
            .map_err(|e| format!("Failed to send fan config to server: {}", e))?;
        
        // Step 2.6: Send bed mesh data to server
        if let Some(bed_mesh) = &configs_corrected.hardware.bed_mesh {
            log::info!("📤 Sending bed mesh data to server...");
            Self::send_mesh_to_server(client, bed_mesh).await
                .map_err(|e| format!("Failed to send mesh data to server: {}", e))?;
        }
        
        // Step 3: Send hardware config frames to device
        log::info!("📤 Sending hardware config to device...");
        let config_frames = ConfigFrameBuilder::build_config_frames(&printer_config);
        
        for frame_bytes in config_frames.iter() {
            client.serial_send_raw(frame_bytes).await
                .map_err(|e| format!("Failed to send config frame to device: {}", e))?;
            // Small delay between frames
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        
        // Step 4: Send ConfigComplete
        client.serial_config_complete().await
            .map_err(|e| format!("Failed to send ConfigComplete: {}", e))?;
        
        // Step 5: Update cached config and get callbacks
        let callbacks = {
            let mut inner = self.inner.write().map_err(|e| format!("Lock error: {}", e))?;
            inner.printer_config = Some(printer_config.clone());
            inner.loaded_configs = Some(configs_corrected);
            inner.callbacks.clone()
        };
        
        // Step 6: Notify all registered callbacks
        Self::notify_callbacks(&callbacks, &printer_config);
        
        log::info!("✅ Configuration reloaded successfully");
        Ok(())
    }

    /// Get the current printer configuration.
    /// 
    /// Returns a clone of the configuration. This allows modules to read
    /// configuration without holding a lock.
    /// 
    /// # Returns
    /// * `Ok(PrinterJsonConfig)` if configuration is loaded
    /// * `Err(String)` if configuration has not been loaded
    pub fn get_config(&self) -> Result<PrinterJsonConfig, String> {
        let inner = self.inner.read().map_err(|e| format!("Lock error: {}", e))?;
        inner.printer_config.clone().ok_or_else(|| "Configuration not loaded. Call load() first.".to_string())
    }

    /// Get the motion configuration as JSON string.
    /// 
    /// This is used to send motion configuration to the server.
    /// 
    /// # Returns
    /// * `Ok(String)` JSON string of motion configuration
    /// * `Err(String)` if configuration has not been loaded
    pub fn get_motion_config_json(&self) -> Result<String, String> {
        let inner = self.inner.read().map_err(|e| format!("Lock error: {}", e))?;
        let configs = inner.loaded_configs.as_ref()
            .ok_or_else(|| "Configuration not loaded. Call load() first.".to_string())?;
        build_motion_config_json(configs)
    }

    /// Get the fan configuration.
    /// 
    /// This is used to send fan index to GPIO name mapping to the server.
    /// 
    /// # Returns
    /// * `Ok(FanConfig)` if configuration is loaded
    /// * `Err(String)` if configuration has not been loaded
    pub fn get_fan_config(&self) -> Result<emb_api::FanConfig, String> {
        let inner = self.inner.read().map_err(|e| format!("Lock error: {}", e))?;
        let configs = inner.loaded_configs.as_ref()
            .ok_or_else(|| "Configuration not loaded. Call load() first.".to_string())?;
        
        let fans = configs.hardware.fan.as_ref()
            .map(|fan_list| {
                fan_list.iter()
                    .map(|f| emb_api::FanConfigEntry {
                        index: f.index,
                        name: f.name.clone(),
                        description: f.description.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        
        Ok(emb_api::FanConfig { fans })
    }

    /// Get the configuration directory path.
    pub fn get_config_dir(&self) -> Result<String, String> {
        let inner = self.inner.read().map_err(|e| format!("Lock error: {}", e))?;
        Ok(inner.config_dir.clone())
    }

    /// Check if configuration has been loaded.
    pub fn is_loaded(&self) -> bool {
        self.inner.read().map(|inner| inner.printer_config.is_some()).unwrap_or(false)
    }

    /// Update and save printer configuration to file.
    ///
    /// This updates the cached configuration and writes it back to printer.json.
    /// Only printer.json is updated; other config files remain unchanged.
    ///
    /// # Arguments
    /// * `updated_config` - The updated printer configuration
    ///
    /// # Returns
    /// * `Ok(())` if save was successful
    /// * `Err(String)` if any error occurred
    pub fn save_printer_config(&self, updated_config: &PrinterJsonConfig) -> Result<(), String> {
        log::info!("💾 Saving printer configuration...");

        // Get config directory
        let config_dir = {
            let inner = self.inner.read().map_err(|e| format!("Lock error: {}", e))?;
            inner.config_dir.clone()
        };

        if config_dir.is_empty() {
            return Err("Configuration not loaded. Call load() first.".to_string());
        }

        // Build printer.json path
        let printer_json_path = std::path::Path::new(&config_dir).join("printer.json");

        // Serialize configuration to JSON
        let json_content = serde_json::to_string_pretty(updated_config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        // Write to file
        std::fs::write(&printer_json_path, json_content)
            .map_err(|e| format!("Failed to write config file: {}", e))?;

        // Update cached config and get callbacks
        let callbacks = {
            let mut inner = self.inner.write().map_err(|e| format!("Lock error: {}", e))?;
            inner.printer_config = Some(updated_config.clone());
            inner.callbacks.clone()
        };

        // Notify all registered callbacks
        Self::notify_callbacks(&callbacks, updated_config);

        log::info!("✅ Printer configuration saved to: {}", printer_json_path.display());
        Ok(())
    }

    /// Update temperature presets in the configuration.
    ///
    /// This is a convenience method that updates only the temperature_presets field
    /// and saves the configuration to temperature.json.
    ///
    /// # Arguments
    /// * `presets` - New temperature presets to save
    ///
    /// # Returns
    /// * `Ok(())` if save was successful
    /// * `Err(String)` if any error occurred
    pub fn save_temperature_presets(
        &self,
        presets: &[super::printer_config::TemperaturePresetConfig],
    ) -> Result<(), String> {
        log::info!("💾 Saving temperature presets...");

        // Get config directory and loaded configs
        let (config_dir, mut loaded_configs) = {
            let inner = self.inner.read().map_err(|e| format!("Lock error: {}", e))?;
            let configs = inner.loaded_configs.clone()
                .ok_or_else(|| "Configuration not loaded. Call load() first.".to_string())?;
            (inner.config_dir.clone(), configs)
        };

        if config_dir.is_empty() {
            return Err("Configuration not loaded. Call load() first.".to_string());
        }

        // Update presets in loaded_configs.temperature
        loaded_configs.temperature.presets = presets.iter().map(|p| {
            super::config_adapter::TemperaturePresetFile {
                name: p.name.clone(),
                hotend_temp: p.hotend_temp,
                bed_temp: p.bed_temp,
                chamber_temp: p.chamber_temp,
                fan_speed: p.fan_speed,
            }
        }).collect();

        // Build temperature.json path
        let temperature_json_path = std::path::Path::new(&config_dir).join("temperature.json");

        // Serialize temperature config to JSON
        let json_content = serde_json::to_string_pretty(&loaded_configs.temperature)
            .map_err(|e| format!("Failed to serialize temperature config: {}", e))?;

        // Write to file
        std::fs::write(&temperature_json_path, json_content)
            .map_err(|e| format!("Failed to write temperature config file: {}", e))?;

        // Rebuild printer config from updated loaded configs
        let printer_config = super::config_adapter::build_printer_config(&loaded_configs);

        // Update cached config and get callbacks
        let callbacks = {
            let mut inner = self.inner.write().map_err(|e| format!("Lock error: {}", e))?;
            inner.printer_config = Some(printer_config.clone());
            inner.loaded_configs = Some(loaded_configs);
            inner.callbacks.clone()
        };

        // Notify all registered callbacks
        Self::notify_callbacks(&callbacks, &printer_config);

        log::info!("✅ Temperature presets saved to: {}", temperature_json_path.display());
        Ok(())
    }

    /// Update PID parameters for a heater in hardware.json.
    ///
    /// This method updates the PID parameters (Kp, Ki, Kd) for the specified heater
    /// in the hardware.json configuration file.
    ///
    /// # Arguments
    /// * `heater` - Heater name ("hotend" or "bed")
    /// * `kp` - Proportional gain
    /// * `ki` - Integral gain
    /// * `kd` - Derivative gain
    ///
    /// # Returns
    /// * `Ok(())` if update was successful
    /// * `Err(String)` if any error occurred
    pub fn update_temperature_pid(
        &self,
        heater: &str,
        kp: f32,
        ki: f32,
        kd: f32,
    ) -> Result<(), String> {
        log::info!("🔧 Updating PID parameters for {}: Kp={:.3}, Ki={:.3}, Kd={:.3}", heater, kp, ki, kd);

        // Get config directory and loaded configs
        let (config_dir, mut loaded_configs) = {
            let inner = self.inner.read().map_err(|e| format!("Lock error: {}", e))?;
            let configs = inner.loaded_configs.clone()
                .ok_or_else(|| "Configuration not loaded. Call load() first.".to_string())?;
            (inner.config_dir.clone(), configs)
        };

        if config_dir.is_empty() {
            return Err("Configuration not loaded. Call load() first.".to_string());
        }

        // Update PID in loaded_configs.hardware.temperature
        let temperature = loaded_configs.hardware.temperature.as_mut()
            .ok_or_else(|| "Temperature config not found in hardware.json".to_string())?;
        
        match heater {
            "hotend" => {
                temperature.hotend.kp = kp;
                temperature.hotend.ki = ki;
                temperature.hotend.kd = kd;
            }
            "bed" => {
                temperature.hotbed.kp = kp;
                temperature.hotbed.ki = ki;
                temperature.hotbed.kd = kd;
            }
            _ => {
                return Err(format!("Unknown heater: {}", heater));
            }
        }

        // Build hardware.json path
        let hardware_json_path = std::path::Path::new(&config_dir).join("hardware.json");

        // Serialize hardware config to JSON
        let json_content = serde_json::to_string_pretty(&loaded_configs.hardware)
            .map_err(|e| format!("Failed to serialize hardware config: {}", e))?;

        // Write to file
        std::fs::write(&hardware_json_path, json_content)
            .map_err(|e| format!("Failed to write hardware config file: {}", e))?;

        // Rebuild printer config from updated loaded configs
        let printer_config = build_printer_config(&loaded_configs);

        // Update cached config and get callbacks
        let callbacks = {
            let mut inner = self.inner.write().map_err(|e| format!("Lock error: {}", e))?;
            inner.printer_config = Some(printer_config.clone());
            inner.loaded_configs = Some(loaded_configs);
            inner.callbacks.clone()
        };

        // Notify all registered callbacks
        Self::notify_callbacks(&callbacks, &printer_config);

        log::info!("✅ PID parameters updated in: {}", hardware_json_path.display());
        Ok(())
    }

    /// Notify all registered callbacks with the new configuration.
    fn notify_callbacks(callbacks: &[Arc<ConfigChangeCallback>], config: &PrinterJsonConfig) {
        if callbacks.is_empty() {
            return;
        }
        
        log::debug!("📢 Notifying {} callback(s) of config change", callbacks.len());
        for callback in callbacks {
            callback(config);
        }
    }
    
    /// Send bed mesh data to server via Socket protocol.
    /// 
    /// Implements Nack retry mechanism with up to 3 attempts:
    /// - MissingSeqs → resend only missing chunks, then retry SetMeshEnd
    /// - Other Nack/errors → full retry from SetMeshBegin
    /// 
    /// # Arguments
    /// * `client` - The CoreSocketClient for communication
    /// * `bed_mesh` - The bed mesh configuration
    /// 
    /// # Returns
    /// * `Ok(())` if mesh data was sent successfully
    /// * `Err(String)` if all retries exhausted
    async fn send_mesh_to_server(client: &CoreSocketClient, bed_mesh: &BedMeshHardwareConfig) -> Result<(), String> {
        // Extract mesh parameters from probe config
        let x_count = bed_mesh.probe.probe_count_x;
        let y_count = bed_mesh.probe.probe_count_y;
        let x_min = bed_mesh.probe.mesh_min_x;
        let x_max = bed_mesh.probe.mesh_max_x;
        let y_min = bed_mesh.probe.mesh_min_y;
        let y_max = bed_mesh.probe.mesh_max_y;
        
        // Extract algorithm parameters from bed_mesh config
        let mesh_pps_x = bed_mesh.algorithm.mesh_pps_x;
        let mesh_pps_y = bed_mesh.algorithm.mesh_pps_y;
        let fade_start = bed_mesh.algorithm.fade_start;
        let fade_end = bed_mesh.algorithm.fade_end;
        let probe_z_adjust = bed_mesh.algorithm.probe_z_adjust;
        
        // Extract mesh points data
        let points = &bed_mesh.data.points;
        
        // Validate points count
        let expected_count = x_count as usize * y_count as usize;
        if points.len() != expected_count {
            return Err(format!(
                "Mesh points count mismatch: expected {} ({}x{}), got {}",
                expected_count, x_count, y_count, points.len()
            ));
        }
        
        log::info!("📊 Mesh grid: {}x{}, range: X({:.1}-{:.1}), Y({:.1}-{:.1}), mesh_pps: {}x{}, fade: {:.1}-{:.1}, probe_z_adjust: {:.3}",
            x_count, y_count, x_min, x_max, y_min, y_max,
            mesh_pps_x, mesh_pps_y, fade_start, fade_end, probe_z_adjust);
        
        // Convert points to binary format (f32 → bytes, big-endian)
        let mut all_data: Vec<u8> = Vec::with_capacity(points.len() * 4);
        for point in points {
            let bytes = point.to_be_bytes();
            all_data.extend_from_slice(&bytes);
        }
        
        // Pre-calculate CRC32 checksum (reused across retries)
        let checksum = Self::calculate_crc32(&all_data);
        
        // Chunk parameters
        const CHUNK_SIZE: usize = 128;
        let total_chunks = (all_data.len() + CHUNK_SIZE - 1) / CHUNK_SIZE;
        
        const MAX_RETRIES: u8 = 3;
        
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                log::warn!("🔄 Mesh transfer retry {}/{}", attempt + 1, MAX_RETRIES);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            
            // === Step 1: SetMeshBegin ===
            let begin_request = CoreRequest::Motion(MotionRequest::SetMeshBegin {
                x_count,
                y_count,
                x_min,
                x_max,
                y_min,
                y_max,
                mesh_pps_x,
                mesh_pps_y,
                fade_start,
                fade_end,
                probe_z_adjust,
            });
            
            match client.send_request(&begin_request).await? {
                CoreResponse::Motion(MotionResponse::Acknowledged) => {
                }
                CoreResponse::Error(e) => {
                    log::warn!("SetMeshBegin failed: {}", e.message);
                    continue; // full retry from top
                }
                other => {
                    log::warn!("Unexpected response to SetMeshBegin: {:?}", other);
                    continue;
                }
            }
            
            // === Step 2: Send all chunks ===
            
            let mut send_ok = true;
            for seq in 0..total_chunks {
                let start = seq * CHUNK_SIZE;
                let end = std::cmp::min(start + CHUNK_SIZE, all_data.len());
                let chunk_data = all_data[start..end].to_vec();
                
                let chunk_request = CoreRequest::Motion(MotionRequest::SetMeshChunk {
                    seq: seq as u16,
                    total: total_chunks as u16,
                    data: chunk_data,
                });
                
                match client.send_request(&chunk_request).await? {
                    CoreResponse::Motion(MotionResponse::MeshAck { .. } | MotionResponse::Acknowledged) => {
                    }
                    CoreResponse::Error(e) => {
                        log::warn!("SetMeshChunk {} failed: {}", seq, e.message);
                        send_ok = false;
                        break;
                    }
                    other => {
                        log::warn!("Unexpected response to SetMeshChunk: {:?}", other);
                        send_ok = false;
                        break;
                    }
                }
                
                // Small delay between chunks (optional, for stability)
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            
            if !send_ok {
                continue; // full retry from SetMeshBegin
            }
            
            // === Step 3: SetMeshEnd with CRC32 ===
            
            let end_request = CoreRequest::Motion(MotionRequest::SetMeshEnd {
                checksum,
            });
            
            match client.send_request(&end_request).await? {
                CoreResponse::Motion(MotionResponse::MeshComplete) => {
                    log::info!("✅ Mesh data transfer complete");
                    return Ok(());
                }
                CoreResponse::Motion(MotionResponse::MeshNack { reason: NackReason::MissingSeqs, missing_seqs, .. }) => {
                    log::warn!("MeshNack: MissingSeqs {:?}, attempting recovery...", missing_seqs);
                    
                    // Recovery path: resend only the missing chunks
                    // (server remains in Stale state, buffer is intact)
                    let mut recovery_ok = true;
                    for seq in &missing_seqs {
                        let seq = *seq as usize;
                        if seq >= total_chunks {
                            continue;
                        }
                        let start = seq * CHUNK_SIZE;
                        let end = std::cmp::min(start + CHUNK_SIZE, all_data.len());
                        let chunk_data = all_data[start..end].to_vec();
                        
                        let chunk_request = CoreRequest::Motion(MotionRequest::SetMeshChunk {
                            seq: seq as u16,
                            total: total_chunks as u16,
                            data: chunk_data,
                        });
                        
                        match client.send_request(&chunk_request).await? {
                            CoreResponse::Motion(MotionResponse::MeshAck { .. } | MotionResponse::Acknowledged) => {
                            }
                            other => {
                                log::warn!("Missing chunk {} resend failed: {:?}", seq, other);
                                recovery_ok = false;
                                break;
                            }
                        }
                    }
                    
                    if recovery_ok {
                        // Retry SetMeshEnd after resending missing chunks
                        match client.send_request(&end_request).await? {
                            CoreResponse::Motion(MotionResponse::MeshComplete) => {
                                log::info!("✅ Mesh transfer complete after MissingSeqs recovery");
                                return Ok(());
                            }
                            other => {
                                log::warn!("SetMeshEnd after MissingSeqs recovery failed: {:?}", other);
                                // Fall through to outer retry
                            }
                        }
                    }
                    // Recovery failed → full retry from SetMeshBegin
                }
                CoreResponse::Motion(MotionResponse::MeshNack { reason, .. }) => {
                    log::warn!("MeshNack: {:?}, retrying from SetMeshBegin", reason);
                    // Fall through to outer retry
                }
                CoreResponse::Error(e) => {
                    log::warn!("SetMeshEnd failed: {}", e.message);
                    // Fall through to outer retry
                }
                other => {
                    log::warn!("Unexpected response to SetMeshEnd: {:?}", other);
                    // Fall through to outer retry
                }
            }
        }
        
        Err(format!(
            "Mesh transfer failed after {} retries",
            MAX_RETRIES
        ))
    }
    
    /// Calculate CRC32 checksum for mesh data.
    /// 
    /// Uses the same algorithm as server-side (crc32fast::hash).
    fn calculate_crc32(data: &[u8]) -> u32 {
        // Simple CRC32 implementation (matches crc32fast::hash)
        // In production, we should use crc32fast crate
        // For now, use a simple implementation
        let mut crc = 0xFFFFFFFFu32;
        for byte in data {
            crc ^= *byte as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB88320u32;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc ^ 0xFFFFFFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_instance() {
        let manager1 = ConfigManager::instance();
        let manager2 = ConfigManager::instance();
        // Same instance
        assert!(std::ptr::eq(manager1, manager2));
    }

    #[test]
    fn test_callback_registration() {
        // Use atomic to track callback invocations
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        // Register callback
        ConfigManager::instance().on_config_change(Box::new(move |_config| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        }));
        
        // Note: This test doesn't actually trigger the callback
        // because we don't want to depend on file system
        // In real usage, load() or reload() would trigger it
    }
}
