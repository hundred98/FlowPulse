//! FlowPulse Web Server Library
//!
//! Axum-based REST API and WebSocket server for FlowPulse 3D printer control system.
//! Optimized for embedded Linux environments with limited memory (RAM < 128MB).

pub mod config;
pub mod handlers;
pub mod middleware;
pub mod routes;

use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tokio::sync::broadcast;
use log::info;
use emb_public::state::FrontendDataProvider;
use emb_public::TemperatureManager;

pub use config::WebServerConfig;

/// Web Server state
pub struct WebServerState {
    /// Server configuration
    pub config: WebServerConfig,
    /// Frontend data provider
    pub data_provider: Arc<dyn FrontendDataProvider>,
    /// Broadcast sender for WebSocket updates
    pub broadcast_tx: broadcast::Sender<emb_public::common::WebSocketMessage>,
    /// Temperature manager for temperature control
    pub temperature_manager: Arc<TemperatureManager>,
}

/// Web Server
pub struct WebServer {
    /// Server state
    state: Arc<WebServerState>,
}

impl WebServer {
    /// Create a new web server
    pub fn new(
        config: WebServerConfig,
        data_provider: Arc<dyn FrontendDataProvider>,
        broadcast_tx: broadcast::Sender<emb_public::common::WebSocketMessage>,
        temperature_manager: Arc<TemperatureManager>,
    ) -> Self {
        let state = Arc::new(WebServerState {
            config,
            data_provider,
            broadcast_tx,
            temperature_manager,
        });
        
        Self { state }
    }

    /// Build the Axum router
    fn build_router(&self) -> Router {
        use axum::middleware;
        
        let mut router = Router::new()
            // Health check (no auth required)
            .route("/health", get(|| async { "OK" }))
            
            // WebSocket route (no auth required for now)
            .route("/ws", get(handlers::websocket::ws_handler))
            
            // Authentication routes (no auth required)
            .route("/api/v1/auth/login", post(handlers::auth::login))
            .route("/api/v1/auth/validate", post(handlers::auth::validate_token))
            
            // API routes (protected by auth if enabled)
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

        // Add authentication middleware if enabled
        if self.state.config.enable_auth {
            router = router.layer(middleware::from_fn_with_state(
                self.state.clone(),
                crate::middleware::auth::auth_middleware,
            ));
            info!("JWT authentication middleware enabled");
        }

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
    use emb_public::core_client::CoreSocketClient;
    use emb_public::SyncEventPublisher;
    use emb_public::temperature::TemperatureManagerConfig;

    #[test]
    fn test_web_server_creation() {
        let config = WebServerConfig::default();
        let (tx, _rx) = broadcast::channel(16);
        let provider = Arc::new(WebDataProvider::new(tx.clone()));
        
        // Create a mock temperature manager for testing
        let core_client = Arc::new(CoreSocketClient::new("127.0.0.1:9527".to_string()));
        let event_publisher = Arc::new(SyncEventPublisher::new());
        let temp_manager = Arc::new(TemperatureManager::new(
            core_client,
            event_publisher,
            TemperatureManagerConfig::default(),
            None,
        ));
        
        let server = WebServer::new(config, provider, tx, temp_manager);
        assert_eq!(server.state.config.port, 8080);
    }
}