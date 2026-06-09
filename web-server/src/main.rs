//! FlowPulse Web Server Binary Entry Point
//!
//! Standalone web server for FlowPulse 3D printer control system.

use web_server::{WebServer, WebServerConfig};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Main entry point
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();
    
    // Load configuration
    let config = WebServerConfig::default();
    
    // Create broadcast channel for WebSocket updates
    let (broadcast_tx, _broadcast_rx) = broadcast::channel(16);
    
    // Create data provider (using WebDataProvider for now)
    let data_provider = Arc::new(emb_public::state::WebDataProvider::new(broadcast_tx.clone()));
    
    // Create and start web server
    let server = WebServer::new(config, data_provider, broadcast_tx);
    server.start().await?;
    
    Ok(())
}