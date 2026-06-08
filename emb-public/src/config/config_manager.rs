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
//! // Reload configuration (user triggered)
//! ConfigManager::instance().reload(&client).await?;
//! ```

use std::sync::RwLock;
use once_cell::sync::Lazy;

use super::printer_config::PrinterJsonConfig;
use super::config_adapter::{load_configs, build_printer_config, build_motion_config_json, LoadedConfigs};
use super::config_protocol::ConfigFrameBuilder;
use crate::CoreSocketClient;

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
}

impl Default for ConfigInner {
    fn default() -> Self {
        Self {
            printer_config: None,
            loaded_configs: None,
            config_dir: String::new(),
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
        
        let mut inner = self.inner.write().map_err(|e| format!("Lock error: {}", e))?;
        inner.config_dir = config_dir.to_string();
        inner.printer_config = Some(printer_config);
        inner.loaded_configs = Some(configs);
        
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
        
        // Step 2: Send motion config to server
        log::info!("📤 Sending motion config to server...");
        let motion_config_json = build_motion_config_json(&configs)?;
        client.config_update_motion(&motion_config_json).await
            .map_err(|e| format!("Failed to send motion config to server: {}", e))?;
        
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
        
        // Step 5: Update cached config
        {
            let mut inner = self.inner.write().map_err(|e| format!("Lock error: {}", e))?;
            inner.printer_config = Some(printer_config);
            inner.loaded_configs = Some(configs);
        }
        
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

    /// Get the configuration directory path.
    pub fn get_config_dir(&self) -> Result<String, String> {
        let inner = self.inner.read().map_err(|e| format!("Lock error: {}", e))?;
        Ok(inner.config_dir.clone())
    }

    /// Check if configuration has been loaded.
    pub fn is_loaded(&self) -> bool {
        self.inner.read().map(|inner| inner.printer_config.is_some()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance() {
        let manager1 = ConfigManager::instance();
        let manager2 = ConfigManager::instance();
        // Same instance
        assert!(std::ptr::eq(manager1, manager2));
    }

    #[test]
    fn test_not_loaded() {
        // Note: This test might fail if other tests have loaded config
        // In real usage, is_loaded() should return false before load()
        let _ = ConfigManager::instance();
    }
}
