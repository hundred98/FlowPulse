//! FlowPulse Web Server
//!
//! Axum-based REST API and WebSocket server for FlowPulse 3D printer control system.
//! Optimized for embedded Linux environments with limited memory (RAM < 128MB).

mod config;
mod handlers;
mod middleware;
mod routes;

use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use log::info;
use emb_public::state::FrontendDataProvider;

pub use config::WebServerConfig;

/// Web Server state
pub struct WebServerState {
    /// Server configuration
    pub config: WebServerConfig,
    /// Frontend data provider
    pub data_provider: Arc<dyn FrontendDataProvider>,
}

/// Web Server
pub struct WebServer {
    /// Server state
    state: Arc<WebServerState>,
}

impl WebServer {
    /// Create a new web server
    pub fn new(config: WebServerConfig, data_provider: Arc<dyn FrontendDataProvider>) -> Self {
        let state = Arc::new(WebServerState {
            config,
            data_provider,
        });
        
        Self { state }
    }

    /// Build the Axum router
    fn build_router(&self) -> Router {
        let mut router = Router::new()
            // Health check
            .route("/health", get(|| async { "OK" }))
            
            // WebSocket route
            .route("/ws", get(handlers::websocket::ws_handler))
            
            // API routes
            .route("/api/v1/printer/status", get(handlers::printer::get_status))
            .route("/api/v1/printer/start", post(handlers::printer::start_print))
            .route("/api/v1/printer/pause", post(handlers::printer::pause_print))
            .route("/api/v1/printer/resume", post(handlers::printer::resume_print))
            .route("/api/v1/printer/stop", post(handlers::printer::stop_print))
            
            // File management
            .route("/api/v1/files", get(handlers::files::list_files).post(handlers::files::upload_file))
            .route("/api/v1/files/:name", delete(handlers::files::delete_file))
            
            // Temperature control
            .route("/api/v1/temperature/status", get(handlers::temperature::get_temperature))
            .route("/api/v1/temperature/target", post(handlers::temperature::set_temperature))
            
            // Configuration
            .route("/api/v1/config", get(handlers::config::get_config))
            
            .with_state(self.state.clone());

        // Add CORS if enabled
        if self.state.config.enable_cors {
            router = router.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            );
        }

        router
    }

    /// Start the web server
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = self.state.config.server_address();
        let router = self.build_router();
        
        info!("Starting web server on {}", addr);
        info!("Max connections: {}", self.state.config.max_connections);
        info!("CORS enabled: {}", self.state.config.enable_cors);
        info!("Auth enabled: {}", self.state.config.enable_auth);

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        
        axum::serve(listener, router)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emb_public::state::WebDataProvider;
    use tokio::sync::broadcast;

    #[test]
    fn test_web_server_creation() {
        let config = WebServerConfig::default();
        let (tx, _rx) = broadcast::channel(16);
        let provider = Arc::new(WebDataProvider::new(tx));
        let server = WebServer::new(config, provider);
        assert_eq!(server.state.config.port, 8080);
    }
}

/// Main entry point
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();
    
    // Load configuration
    let config = WebServerConfig::default();
    
    // Create data provider (using WebDataProvider for now)
    let (broadcast_tx, _broadcast_rx) = tokio::sync::broadcast::channel(16);
    let data_provider = Arc::new(emb_public::state::WebDataProvider::new(broadcast_tx));
    
    // Create and start web server
    let server = WebServer::new(config, data_provider);
    server.start().await?;
    
    Ok(())
}
