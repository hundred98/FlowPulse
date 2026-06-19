//! FlowPulse Host Application
//!
//! Main service application for 3D printer control.
//! Connects to emb-core-server, manages device state, and provides multi-channel access.

use host::{app::AppState, setup};
use emb_public::config::log_config::LogConfig;
use emb_public::logger;
use emb_public::{GpioManager, HomingManager};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing from logging.json config
    let log_config = LogConfig::load(setup::CONFIG_DIR)
        .unwrap_or_else(|e| {
            eprintln!("Warning: Failed to load logging config: {}, using defaults", e);
            LogConfig::default()
        });
    logger::init_tracing(&log_config)?;

    tracing::info!("========================================");
    tracing::info!("FlowPulse Host Service Starting");
    tracing::info!("========================================");

    tracing::info!("Server: {}", setup::SERVER_ADDR);
    tracing::info!("Config: {}", setup::CONFIG_DIR);

    // Step 1: Load all configuration files at once
    let _printer_config = setup::load_all_configs(setup::CONFIG_DIR)?;

    // Step 2: Create host and connect to emb-core-server
    let host = setup::create_and_connect_host(setup::SERVER_ADDR).await?;

    // Step 2.5: Create GpioManager and setup callback (before GPIO subscribe in initialize_device)
    let gpio_manager = GpioManager::new(host.client());
    gpio_manager.setup_callback().await;
    tracing::info!("✅ GpioManager callback setup");

    // Step 3: Initialize device (serial, GPIO subscribe, configs, STM32 seq)
    setup::initialize_device(&host).await?;

    // Step 3.5: Create HomingManager
    let homing_manager = HomingManager::new(host.client());

    // Step 4: Create application state with all managers
    let app_state = AppState::new(host.client(), gpio_manager, homing_manager);

    // Initialize application state
    app_state.initialize().await?;
    tracing::info!("✅ Application state initialized");

    // Step 5: Start WebServer in background
    let _web_server_handle = setup::start_web_server(&app_state);
    
    // Step 6: Start services (including temperature subscription)
    app_state.start_services().await?;
    tracing::info!("✅ Background services started");

    // Get initial position
    match host.get_position().await {
        Ok((x, y, z, e)) => tracing::info!("Initial position: X={:.3} Y={:.3} Z={:.3} E={:.3}", x, y, z, e),
        Err(e) => tracing::warn!("Get position failed: {}", e),
    }

    tracing::info!("========================================");
    tracing::info!("✅ FlowPulse Host Service Ready");
    tracing::info!("========================================");
    tracing::info!("Web UI:     http://127.0.0.1:8080");
    tracing::info!("WebSocket:  ws://127.0.0.1:8080/ws");
    tracing::info!("UnixSocket: /tmp/flowpulse.sock");
    tracing::info!("Press Ctrl+C to stop");
    tracing::info!("========================================");
    
    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    
    tracing::info!("Shutting down...");
    app_state.stop_services().await?;
    tracing::info!("✅ Services stopped");
    
    host.disconnect().await.ok();

    Ok(())
}


