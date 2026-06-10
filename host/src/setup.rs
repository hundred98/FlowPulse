//! Setup and initialization module
//!
//! This module contains functions for initializing the FlowPulse host application.

use emb_public::{ConfigManager, ConfigFrameBuilder, PrinterJsonConfig};
use emb_public::state::WebDataProvider;
use web_server::{WebServer, WebServerConfig};
use std::sync::Arc;
use tokio::task::JoinHandle;
use crate::printer_host_v2::PrinterHostV2;
use crate::app::AppState;

/// Default socket server address
pub const SERVER_ADDR: &str = "127.0.0.1:9527";

/// Default configuration directory
pub const CONFIG_DIR: &str = "config";

/// Load configuration files and build motion config JSON
///
/// # Arguments
/// * `config_dir` - Path to the configuration directory
///
/// # Returns
/// * `Ok((PrinterJsonConfig, String))` - Printer config and motion config JSON
/// * `Err(anyhow::Error)` - If loading or parsing fails
pub fn load_configuration(config_dir: &str) -> anyhow::Result<(PrinterJsonConfig, String)> {
    // Load config files (hardware.json + motion.json + printer.json)
    ConfigManager::instance().load(config_dir)
        .map_err(|e| anyhow::anyhow!("Failed to load configs: {}", e))?;

    let printer_config = ConfigManager::instance().get_config()
        .map_err(|e| anyhow::anyhow!("Failed to get config: {}", e))?;

    log::info!(
        "Loaded {} motors, printer model: {}",
        printer_config.motor.len(),
        printer_config.printer_model,
    );

    // Build MotionConfig JSON
    let motion_json = ConfigManager::instance().get_motion_config_json()
        .map_err(|e| anyhow::anyhow!("Failed to build motion config: {}", e))?;

    Ok((printer_config, motion_json))
}

/// Initialize device (serial connection, send configs, initialize STM32)
///
/// # Arguments
/// * `host` - PrinterHostV2 instance
/// * `printer_config` - Printer configuration
/// * `motion_json` - Motion config JSON string
///
/// # Returns
/// * `Ok(())` - If initialization succeeds
/// * `Err(anyhow::Error)` - If critical initialization fails
pub async fn initialize_device(
    host: &PrinterHostV2,
    printer_config: &PrinterJsonConfig,
    motion_json: &str,
) -> anyhow::Result<()> {
    // Step 1: Connect serial port to STM32
    let (serial_port, serial_baud) = {
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

    // Step 2: Send MotionConfig to server
    match host.client().config_update_motion(motion_json).await {
        Ok(()) => log::info!("✅ Motion config sent to server"),
        Err(e) => log::warn!("⚠️  Send motion config failed (using defaults): {}", e),
    }

    // Step 3: Send FanConfig to server
    match ConfigManager::instance().get_fan_config() {
        Ok(fan_config) => {
            match host.client().config_update_fan(&fan_config).await {
                Ok(()) => log::info!("✅ Fan config sent to server"),
                Err(e) => log::warn!("⚠️  Send fan config failed: {}", e),
            }
        }
        Err(e) => log::warn!("⚠️  Get fan config failed: {}", e),
    }

    // Step 4: Send config frames to STM32 device
    let config_frames = ConfigFrameBuilder::build_config_frames(printer_config);

    for frame_bytes in config_frames.iter() {
        host.client().serial_send_raw(frame_bytes).await
            .map_err(|e| anyhow::anyhow!("Failed to send config frame: {}", e))?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Step 5: Initialize STM32 device
    match host.client().serial_config_complete().await {
        Ok(()) => log::info!("✅ ConfigComplete sent"),
        Err(e) => log::warn!("⚠️  ConfigComplete failed: {}", e),
    }

    match host.client().serial_init_seq().await {
        Ok(()) => log::info!("✅ Device seq initialized"),
        Err(e) => log::warn!("⚠️  Init seq failed: {}", e),
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
    let data_provider = Arc::new(WebDataProvider::new(websocket_broadcast_tx.clone()));

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
