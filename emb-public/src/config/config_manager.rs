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
use super::config_adapter::{load_configs, build_printer_config, build_motion_config_json, LoadedConfigs};
use super::config_protocol::{ConfigFrameBuilder, validate_config};
use crate::CoreSocketClient;

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
        let printer_config = build_printer_config(&configs);
        
        // Step 1.5: Validate configuration before using it
        validate_config(&printer_config)?;
        
        // Step 2: Send motion config to server
        log::info!("📤 Sending motion config to server...");
        let motion_config_json = build_motion_config_json(&configs)?;
        client.config_update_motion(&motion_config_json).await
            .map_err(|e| format!("Failed to send motion config to server: {}", e))?;
        
        // Step 2.5: Send fan config to server
        log::info!("📤 Sending fan config to server...");
        let fan_config = emb_api::FanConfig {
            fans: configs.hardware.fan.as_ref()
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
            inner.loaded_configs = Some(configs);
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
    /// and saves the configuration.
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
        // Get current config
        let mut config = self.get_config()?;

        // Update presets
        config.temperature_presets = presets.to_vec();

        // Save updated config
        self.save_printer_config(&config)
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
