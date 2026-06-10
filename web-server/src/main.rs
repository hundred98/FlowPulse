//! FlowPulse Web Server Binary Entry Point
//!
//! Standalone web server for FlowPulse 3D printer control system.
//! Note: This is a standalone test/development server without actual temperature control.
//! For production use, run the main host application instead.

use web_server::{WebServer, WebServerConfig};
use std::sync::Arc;
use tokio::sync::broadcast;
use emb_public::TemperatureManager;
use emb_public::core_client::{CoreSocketClient, CoreClientConfig};
use emb_public::SyncEventPublisher;
use emb_public::temperature::TemperatureManagerConfig;

/// Main entry point
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();

    log::info!("Starting standalone web server (development mode)");
    log::warn!("Note: Temperature control will not work in standalone mode");
    log::warn!("For production use, run the main host application instead");

    // Load configuration
    let config = WebServerConfig::default();

    // Create broadcast channel for WebSocket updates
    let (broadcast_tx, _broadcast_rx) = broadcast::channel(16);

    // Create data provider (using WebDataProvider for now)
    let data_provider = Arc::new(emb_public::state::WebDataProvider::new(broadcast_tx.clone()));

    // Create a mock temperature manager for standalone mode
    // Note: This won't actually control temperature - it's just to satisfy the interface
    let core_client = Arc::new(CoreSocketClient::new(CoreClientConfig {
        server_addr: "127.0.0.1:9527".to_string(),
        connect_timeout_ms: 5000,
        request_timeout_ms: 30000,
        auto_reconnect: true,
    }));
    let event_publisher = Arc::new(SyncEventPublisher::new());
    let temp_manager = Arc::new(TemperatureManager::new(
        core_client,
        event_publisher,
        TemperatureManagerConfig::default(),
        None,
    ));

    // Create and start web server
    let server = WebServer::new(config, data_provider, broadcast_tx, temp_manager);
    server.start().await?;

    Ok(())
}