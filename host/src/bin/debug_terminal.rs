//! Debug Terminal Server
//!
//! Simple HTTP server for GPIO debugging.
//! Usage: debug_terminal [server_addr] [core_addr] [serial_port] [baud_rate]
//!
//! Example:
//!   debug_terminal 127.0.0.1:8080 127.0.0.1:9527 COM7 57600
//!
//! Then open http://127.0.0.1:8080/debug in browser.

use std::sync::Arc;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};
use axum::http::Method;

use emb_public::{CoreSocketClient, ConfigFrameBuilder, config_adapter};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GpioSetRequest {
    name: String,
    value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GpioQueryRequest {
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerialConnectRequest {
    port: String,
    baud_rate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GpioInfo {
    name: String,
    value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusInfo {
    serial_connected: bool,
    serial_port: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T> ApiResponse<T> {
    fn success(data: T) -> Self {
        Self { success: true, data: Some(data), error: None }
    }
    fn error(msg: String) -> Self {
        Self { success: false, data: None, error: Some(msg) }
    }
}

struct DebugState {
    core_client: Arc<CoreSocketClient>,
}

fn create_debug_router(state: Arc<DebugState>) -> Router {
    Router::new()
        .route("/", get(debug_page))
        .route("/api/status", get(get_status))
        .route("/api/config/load", post(load_configs))
        .route("/api/serial/connect", post(serial_connect))
        .route("/api/serial/disconnect", post(serial_disconnect))
        .route("/api/gpio/set", get(gpio_set))
        .route("/api/gpio/query", get(gpio_query))
        .with_state(state)
}

async fn debug_page() -> impl IntoResponse {
    Html(include_str!("../debug_terminal/debug.html"))
}

async fn get_status(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    match state.core_client.serial_query_status().await {
        Ok((connected, port)) => Json(ApiResponse::success(StatusInfo {
            serial_connected: connected,
            serial_port: port,
        })),
        Err(e) => Json(ApiResponse::<StatusInfo>::error(format!("Error: {}", e))),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoadConfigsRequest {
    config_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LoadConfigsResult {
    printer_loaded: bool,
    motion_loaded: bool,
    hardware_loaded: bool,
}

async fn load_configs(
    State(state): State<Arc<DebugState>>,
    Json(req): Json<LoadConfigsRequest>,
) -> impl IntoResponse {
    let config_dir = req.config_dir.unwrap_or_else(|| {
        let dir = std::env::current_dir()
            .map(|p| p.join("config"))
            .unwrap_or_else(|_| std::path::PathBuf::from("config"));
        dir.to_string_lossy().to_string()
    });
    
    log::info!("Loading all configs from: {}", config_dir);
    
    match state.core_client.load_all_configs(&config_dir).await {
        Ok((printer, motion, hardware)) => {
            log::info!("Configs loaded: printer={}, motion={}, hardware={}", printer, motion, hardware);
            Json(ApiResponse::success(LoadConfigsResult {
                printer_loaded: printer,
                motion_loaded: motion,
                hardware_loaded: hardware,
            }))
        }
        Err(e) => Json(ApiResponse::<LoadConfigsResult>::error(format!("Load failed: {}", e))),
    }
}

async fn serial_connect(
    State(state): State<Arc<DebugState>>,
    Json(req): Json<SerialConnectRequest>,
) -> impl IntoResponse {
    log::info!("Connecting serial: {} @ {}", req.port, req.baud_rate);
    
    match state.core_client.serial_connect(&req.port, req.baud_rate).await {
        Ok(()) => {
            log::info!("Serial connected to {}", req.port);
            
            let config_dir = std::env::current_dir()
                .map(|p| p.join("config"))
                .unwrap_or_else(|_| std::path::PathBuf::from("config"));
            
            match config_adapter::load_configs(&config_dir.to_string_lossy()) {
                Ok(configs) => {
                    log::info!("Configs loaded successfully");
                    
                    let printer_config = config_adapter::build_printer_config(&configs);
                    let config_frames = ConfigFrameBuilder::build_config_frames(&printer_config);
                    log::info!("Sending {} config frames to device...", config_frames.len());
                    
                    for frame_bytes in &config_frames {
                        match state.core_client.serial_send_raw(frame_bytes).await {
                            Ok(()) => log::debug!("Config frame sent: {} bytes", frame_bytes.len()),
                            Err(e) => log::warn!("Failed to send config frame: {}", e),
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    
                    log::info!("All config frames sent, waiting 300ms before ConfigComplete...");
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    
                    match state.core_client.serial_config_complete().await {
                        Ok(()) => log::info!("ConfigComplete sent"),
                        Err(e) => log::warn!("ConfigComplete failed: {}", e),
                    }
                    
                    match state.core_client.serial_init_seq().await {
                        Ok(()) => log::info!("Device seq initialized"),
                        Err(e) => log::warn!("Init seq failed: {}", e),
                    }
                    
                    match state.core_client.serial_enter_special_mode().await {
                        Ok(()) => log::info!("Entered special mode"),
                        Err(e) => log::warn!("EnterSpecialMode failed: {}", e),
                    }
                }
                Err(e) => log::warn!("Failed to load configs for serial init: {}", e),
            }
            
            Json(ApiResponse::success(StatusInfo {
                serial_connected: true,
                serial_port: Some(req.port),
            }))
        }
        Err(e) => Json(ApiResponse::<StatusInfo>::error(format!("Connect failed: {}", e))),
    }
}

async fn serial_disconnect(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    log::info!("Disconnecting serial");
    
    match state.core_client.serial_disconnect().await {
        Ok(()) => {
            log::info!("Serial disconnected");
            Json(ApiResponse::success(StatusInfo {
                serial_connected: false,
                serial_port: None,
            }))
        }
        Err(e) => Json(ApiResponse::<StatusInfo>::error(format!("Disconnect failed: {}", e))),
    }
}

async fn gpio_set(
    State(state): State<Arc<DebugState>>,
    axum::extract::Query(req): axum::extract::Query<GpioSetRequest>,
) -> impl IntoResponse {
    log::info!("Debug: GPIO set {} = {}", req.name, req.value);
    
    match state.core_client.gpio_set(&req.name, req.value).await {
        Ok(_) => Json(ApiResponse::success(GpioInfo { name: req.name, value: req.value })),
        Err(e) => Json(ApiResponse::<GpioInfo>::error(format!("Error: {}", e))),
    }
}

async fn gpio_query(
    State(state): State<Arc<DebugState>>,
    axum::extract::Query(req): axum::extract::Query<GpioQueryRequest>,
) -> impl IntoResponse {
    log::info!("Debug: GPIO query {}", req.name);
    
    match state.core_client.gpio_query(&req.name).await {
        Ok(value) => Json(ApiResponse::success(GpioInfo { name: req.name, value })),
        Err(e) => Json(ApiResponse::<GpioInfo>::error(format!("Error: {}", e))),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let http_addr = args.get(1).unwrap_or(&"127.0.0.1:8080".to_string()).clone();
    let core_addr = args.get(2).unwrap_or(&"127.0.0.1:9527".to_string()).clone();
    let serial_port = args.get(3).cloned();
    let baud_rate: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(57600);

    log::info!("Debug Terminal starting...");
    log::info!("HTTP server: {}", http_addr);
    log::info!("Core server: {}", core_addr);

    let core_client = Arc::new(CoreSocketClient::default_client(&core_addr));
    
    match core_client.connect().await {
        Ok(()) => log::info!("Connected to core server"),
        Err(e) => {
            log::error!("Failed to connect to core server: {}", e);
            log::error!("Make sure emb-core-server is running at {}", core_addr);
            std::process::exit(1);
        }
    }

    // Load all configs
    let config_dir = std::env::current_dir()
        .map(|p| p.join("config"))
        .unwrap_or_else(|_| std::path::PathBuf::from("config"));
    
    log::info!("Loading configs from: {}", config_dir.display());
    match core_client.load_all_configs(&config_dir.to_string_lossy()).await {
        Ok((printer, motion, hardware)) => {
            log::info!("Configs loaded: printer={}, motion={}, hardware={}", printer, motion, hardware);
        }
        Err(e) => log::warn!("Failed to load configs: {}", e),
    }

    if let Some(port) = serial_port {
        log::info!("Auto-connecting serial: {} @ {}", port, baud_rate);
        match core_client.serial_connect(&port, baud_rate).await {
            Ok(()) => log::info!("Serial connected to {}", port),
            Err(e) => log::error!("Serial connect failed: {}", e),
        }
    }

    let state = Arc::new(DebugState { core_client });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    let app = Router::new()
        .nest("/debug", create_debug_router(state))
        .layer(cors);

    let listener = tokio::net::TcpListener::bind(&http_addr).await?;
    log::info!("Debug terminal available at http://{}/debug", http_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
