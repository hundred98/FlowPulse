//! Setup and initialization module
//!
//! This module contains functions for initializing the FlowPulse host application.

use emb_public::{ConfigManager, PrinterJsonConfig, CoreSocketClient};
use emb_public::config::ConfigFrameBuilder;
use emb_public::state::WebDataProvider;
use emb_public::temperature::TemperatureManager;
use web_server::{WebServer, WebServerConfig};
use std::sync::Arc;
use tokio::task::JoinHandle;
use crate::printer_host_v2::PrinterHostV2;
use crate::app::AppState;

/// Default socket server address
pub const SERVER_ADDR: &str = "127.0.0.1:9527";

/// Default configuration directory
pub const CONFIG_DIR: &str = "config";

/// Load all configuration files at once
///
/// Calls `ConfigManager::load()` to read all config files
/// (hardware.json, motion.json, printer.json, temperature.json, etc.)
/// and returns the complete `PrinterJsonConfig`.
///
/// # Arguments
/// * `config_dir` - Path to the configuration directory
///
/// # Returns
/// * `Ok(PrinterJsonConfig)` - Complete printer configuration
/// * `Err(anyhow::Error)` - If loading or parsing fails
pub fn load_all_configs(config_dir: &str) -> anyhow::Result<PrinterJsonConfig> {
    // Load all config files (hardware.json + motion.json + printer.json + temperature.json + ...)
    ConfigManager::instance().load(config_dir)
        .map_err(|e| anyhow::anyhow!("Failed to load configs: {}", e))?;

    let printer_config = ConfigManager::instance().get_config()
        .map_err(|e| anyhow::anyhow!("Failed to get config: {}", e))?;

    log::info!(
        "Loaded {} motors, printer model: {}",
        printer_config.motor.len(),
        printer_config.printer_model,
    );

    Ok(printer_config)
}

/// Send all configs to server and device at once
///
/// This function uses ConfigManager::reload() to send:
/// - Motion config to server
/// - Fan config to server
/// - Mesh data to server (if available)
/// - Hardware config to device
/// - ConfigComplete to device
///
/// Must be called after `load_all_configs()` and serial connection.
///
/// # Arguments
/// * `client` - CoreSocketClient instance
///
/// # Returns
/// * `Ok(())` - If all configs sent successfully
/// * `Err(anyhow::Error)` - If sending fails
pub async fn send_all_configs(client: &CoreSocketClient) -> anyhow::Result<()> {
    log::info!("Sending all configs to server and device...");
    ConfigManager::instance().reload(client).await
        .map_err(|e| anyhow::anyhow!("Failed to send configs: {}", e))?;
    log::info!("All configs sent (including Mesh data if available)");
    Ok(())
}

/// Initialize temperature manager after configuration is loaded and serial is connected
///
/// This function:
/// 1. Loads heater parameters (bed, hotend) from ConfigManager
/// 2. Subscribes to temperature updates from the device
///
/// # Arguments
/// * `temperature_manager` - TemperatureManager instance (typically from AppState)
///
/// # Returns
/// * `Ok(())` - If initialization succeeds
/// * `Err(anyhow::Error)` - If initialization fails
pub async fn initialize_temperature_manager(temperature_manager: &TemperatureManager) -> anyhow::Result<()> {
    log::info!("Initializing temperature manager...");
    temperature_manager.initialize().await
        .map_err(|e| anyhow::anyhow!("Failed to initialize temperature manager: {}", e))?;
    temperature_manager.subscribe_temperature_updates().await
        .map_err(|e| anyhow::anyhow!("Failed to subscribe temperature updates: {}", e))?;
    log::info!("Temperature manager initialized");
    Ok(())
}

/// Subscribe to GPIO report events after serial connection
///
/// Enables the core server to forward GPIO status reports from the device.
///
/// # Arguments
/// * `client` - CoreSocketClient instance
///
/// # Returns
/// * `Ok(())` - If subscription succeeds
/// * `Err(anyhow::Error)` - If subscription fails
pub async fn subscribe_gpio_report(client: &CoreSocketClient) -> anyhow::Result<()> {
    client.gpio_subscribe_report(true).await
        .map_err(|e| anyhow::anyhow!("Failed to subscribe GPIO report: {}", e))?;
    log::info!("Subscribed to GPIO report");
    Ok(())
}

/// Initialize device (serial connection, send configs, initialize STM32)
///
/// This function now uses ConfigManager::reload() to send all configs,
/// including Mesh data for bed compensation.
///
/// # Arguments
/// * `host` - PrinterHostV2 instance
///
/// # Returns
/// * `Ok(())` - If initialization succeeds
/// * `Err(anyhow::Error)` - If critical initialization fails
pub async fn initialize_device(host: &PrinterHostV2) -> anyhow::Result<()> {
    // Step 1: Connect serial port to STM32
    let (serial_port, serial_baud) = {
        let printer_config = ConfigManager::instance().get_config()
            .map_err(|e| anyhow::anyhow!("Failed to get config: {}", e))?;
        let serial = &printer_config.communication.serial;
        (serial.port.clone(), serial.baud_rate)
    };

    log::info!("Connecting serial {} @ {} baud...", serial_port, serial_baud);
    match host.client().serial_connect(&serial_port, serial_baud).await {
        Ok(()) => log::info!("✅ Serial connected to {}", serial_port),
        Err(e) => {
            log::error!("❌ Serial connect failed: {}", e);
            log::error!("Continuing in plan-only mode (no motor movement)");
        }
    }

    // Step 2: Subscribe to GPIO report events
    match subscribe_gpio_report(&host.client()).await {
        Ok(()) => log::info!("✅ GPIO report subscribed"),
        Err(e) => log::warn!("⚠️  GPIO report subscribe failed: {}", e),
    }

    // Step 3: Send all configs to server and device
    match send_all_configs(&host.client()).await {
        Ok(()) => log::info!("✅ All configs sent (including Mesh data if available)"),
        Err(e) => log::warn!("⚠️  Send configs failed: {}", e),
    }

    // Step 4: Initialize STM32 device (seq reset)
    match host.client().serial_init_seq().await {
        Ok(()) => log::info!("✅ Device seq initialized"),
        Err(e) => log::warn!("⚠️  Init seq failed: {}", e),
    }

    // Step 5: 发送 StatusQuery 帧 (0x03)，触发下位机立即上报状态并激活定时上报
    let query_frame = ConfigFrameBuilder::build_status_query_frame();
    match host.client().serial_send_raw(&query_frame).await {
        Ok(()) => log::info!("✅ StatusQuery sent (trigger device to start periodic reporting)"),
        Err(e) => log::warn!("⚠️  StatusQuery send failed: {}", e),
    }

    Ok(())
}

/// Start WebServer in background
///
/// # Arguments
/// * `app_state` - Application state containing temperature manager and broadcast channel
///
/// # Returns
/// * `JoinHandle<()>` - Handle to the background task running the WebServer
pub fn start_web_server(app_state: &AppState) -> JoinHandle<()> {
    // Create WebDataProvider with broadcast channel
    let websocket_broadcast_tx = app_state.websocket_broadcast_tx.clone();
    let core_client = app_state.core_client.clone();
    let data_provider = Arc::new(WebDataProvider::new(
        websocket_broadcast_tx.clone(),
        core_client,
    ));

    // Create and start WebServer with temperature manager
    let web_config = WebServerConfig::default();
    let web_server = WebServer::new(
        web_config,
        data_provider,
        websocket_broadcast_tx,
        app_state.temperature_manager.clone(),
    );

    // Start WebServer in background
    tokio::spawn(async move {
        if let Err(e) = web_server.start().await {
            log::error!("WebServer error: {}", e);
        }
    })
}

/// Create PrinterHostV2 and connect to emb-core-server
///
/// # Arguments
/// * `server_addr` - Socket server address (e.g., "127.0.0.1:9527")
///
/// # Returns
/// * `Ok(PrinterHostV2)` - Connected host instance
/// * `Err(anyhow::Error)` - If connection fails
pub async fn create_and_connect_host(server_addr: &str) -> anyhow::Result<PrinterHostV2> {
    use crate::printer_host_v2::HostV2Config;

    // Create host
    let host_config = HostV2Config {
        server_addr: server_addr.to_string(),
        ..Default::default()
    };
    let host = PrinterHostV2::new(host_config);

    // Connect to emb-core-server
    log::info!("Connecting to emb-core-server...");
    host.connect_socket().await
        .map_err(|e| anyhow::anyhow!("TCP connection failed: {}", e))?;
    log::info!("✅ Connected to emb-core-server");

    Ok(host)
}
