//! FlowPulse Host Application
//!
//! Main service application for 3D printer control.
//! Connects to emb-core-server, manages device state, and provides multi-channel access.

mod printer_host_v2;
mod app;
mod setup;

use app::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("========================================");
    log::info!("FlowPulse Host Service Starting");
    log::info!("========================================");

    log::info!("Server: {}", setup::SERVER_ADDR);
    log::info!("Config: {}", setup::CONFIG_DIR);

    // Step 1: Load configuration files
    let (printer_config, motion_json) = setup::load_configuration(setup::CONFIG_DIR)?;

    // Step 2: Create host and connect to emb-core-server
    let host = setup::create_and_connect_host(setup::SERVER_ADDR).await?;

    // Step 3: Initialize device (serial, configs, STM32)
    setup::initialize_device(&host, &printer_config, &motion_json).await?;

    // Step 4: Create application state
    let app_state = AppState::new(host.client());

    // Initialize application state
    app_state.initialize().await?;
    log::info!("✅ Application state initialized");

    // Step 5: Start WebServer in background
    let _web_server_handle = setup::start_web_server(&app_state);
    
    // Step 6: Start services (including temperature subscription)
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
