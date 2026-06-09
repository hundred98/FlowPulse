//! FlowPulse Host Application
//!
//! Main service application for 3D printer control.
//! Connects to emb-core-server, manages device state, and provides multi-channel access.

mod printer_host_v2;
mod app;

use printer_host_v2::{PrinterHostV2, HostV2Config};
use app::AppState;
use emb_public::ConfigManager;
use emb_public::ConfigFrameBuilder;
use emb_public::state::WebDataProvider;
use web_server::{WebServer, WebServerConfig};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("========================================");
    log::info!("FlowPulse Host Service Starting");
    log::info!("========================================");

    // Configuration
    let server_addr = "127.0.0.1:9527";
    let config_dir = "config";

    log::info!("Server: {}", server_addr);
    log::info!("Config: {}", config_dir);

    // Load config files (hardware.json + motion.json + printer.json)
    ConfigManager::instance().load(config_dir)
        .unwrap_or_else(|e| {
            log::error!("Failed to load configs: {}", e);
            std::process::exit(1);
        });

    let printer_config = ConfigManager::instance().get_config()
        .unwrap_or_else(|e| {
            log::error!("Failed to get config: {}", e);
            std::process::exit(1);
        });

    log::info!(
        "Loaded {} motors, printer model: {}",
        printer_config.motor.len(),
        printer_config.printer_model,
    );

    // Build MotionConfig JSON
    let motion_json = ConfigManager::instance().get_motion_config_json().unwrap_or_else(|e| {
        log::error!("Failed to build motion config: {}", e);
        std::process::exit(1);
    });

    // Create host
    let host_config = HostV2Config {
        server_addr: server_addr.to_string(),
        ..Default::default()
    };
    let host = PrinterHostV2::new(host_config);

    // Step 1: Connect to emb-core-server
    log::info!("Connecting to emb-core-server...");
    match host.connect_socket().await {
        Ok(()) => log::info!("✅ Connected to emb-core-server"),
        Err(e) => {
            log::error!("❌ TCP connection failed: {}", e);
            std::process::exit(1);
        }
    }

    // Step 2: Connect serial port to STM32
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

    // Step 3: Send MotionConfig to server
    match host.client().config_update_motion(&motion_json).await {
        Ok(()) => log::info!("✅ Motion config sent to server"),
        Err(e) => log::warn!("⚠️  Send motion config failed (using defaults): {}", e),
    }

    // Step 4: Send FanConfig to server
    match ConfigManager::instance().get_fan_config() {
        Ok(fan_config) => {
            match host.client().config_update_fan(&fan_config).await {
                Ok(()) => log::info!("✅ Fan config sent to server"),
                Err(e) => log::warn!("⚠️  Send fan config failed: {}", e),
            }
        }
        Err(e) => log::warn!("⚠️  Get fan config failed: {}", e),
    }

    // Step 5: Send config frames to STM32 device
    let config_frames = ConfigFrameBuilder::build_config_frames(&printer_config);
    log::info!("Sending {} config frames to device...", config_frames.len());

    for frame_bytes in &config_frames {
        match host.client().serial_send_raw(frame_bytes).await {
            Ok(()) => log::debug!("Config frame sent: {} bytes", frame_bytes.len()),
            Err(e) => log::warn!("Failed to send config frame: {}", e),
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    log::info!("All config frames sent, waiting 300ms before ConfigComplete...");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Step 6: Initialize STM32 device
    match host.client().serial_config_complete().await {
        Ok(()) => log::info!("✅ ConfigComplete sent"),
        Err(e) => log::warn!("⚠️  ConfigComplete failed: {}", e),
    }

    match host.client().serial_init_seq().await {
        Ok(()) => log::info!("✅ Device seq initialized"),
        Err(e) => log::warn!("⚠️  Init seq failed: {}", e),
    }

    // Step 7: Create application state
    let app_state = AppState::new(host.client());
    
    // Initialize application state
    app_state.initialize().await?;
    log::info!("✅ Application state initialized");
    
    // Create WebDataProvider with broadcast channel
    let websocket_broadcast_tx = app_state.websocket_broadcast_tx.clone();
    let data_provider = Arc::new(WebDataProvider::new(websocket_broadcast_tx.clone()));
    
    // Create and start WebServer
    let web_config = WebServerConfig::default();
    let web_server = WebServer::new(web_config, data_provider, websocket_broadcast_tx);
    
    // Start WebServer in background
    tokio::spawn(async move {
        if let Err(e) = web_server.start().await {
            log::error!("WebServer error: {}", e);
        }
    });
    
    // Start services (including temperature subscription)
    app_state.start_services().await?;
    log::info!("✅ Background services started");

    // Get initial position
    match host.get_position().await {
        Ok((x, y, z, e)) => log::info!("Initial position: X={:.3} Y={:.3} Z={:.3} E={:.3}", x, y, z, e),
        Err(e) => log::warn!("Get position failed: {}", e),
    }

    log::info!("========================================");
    log::info!("✅ FlowPulse Host Service Ready");
    log::info!("========================================");
    log::info!("Web UI:     http://127.0.0.1:8080");
    log::info!("WebSocket:  ws://127.0.0.1:8080/ws");
    log::info!("UnixSocket: /tmp/flowpulse.sock");
    log::info!("Press Ctrl+C to stop");
    log::info!("========================================");
    
    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    
    log::info!("Shutting down...");
    app_state.stop_services().await?;
    log::info!("✅ Services stopped");
    
    host.disconnect().await.ok();

    Ok(())
}
