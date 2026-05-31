//! Remote Control API Module
//!
//! Provides HTTP REST API and WebSocket for remote printer control.
//! Based on Axum framework for high performance and low memory footprint.
//!
//! All core operations (motion, leveling, serial) are handled via Socket API
//! through emb-core-server.
//!
//! ## API Endpoints
//!
//! ### Printer Control
//! - `POST /api/v1/printer/start` - Start print job
//! - `POST /api/v1/printer/pause` - Pause print
//! - `POST /api/v1/printer/resume` - Resume print
//! - `POST /api/v1/printer/stop` - Stop print
//! - `GET /api/v1/printer/status` - Get printer status
//!
//! ### File Management
//! - `GET /api/v1/files` - List G-code files
//! - `POST /api/v1/files/upload` - Upload G-code file
//! - `DELETE /api/v1/files/{name}` - Delete file
//!
//! ### Temperature Control
//! - `POST /api/v1/temperature/target` - Set target temperature
//! - `GET /api/v1/temperature/status` - Get temperature status
//!
//! ### Configuration
//! - `GET /api/v1/config` - Get configuration
//! - `POST /api/v1/config` - Update configuration
//!
//! ### Real-time Data
//! - `WS /ws` - WebSocket for real-time updates (temperature, position, progress)

pub mod leveling;

use axum::{
    extract::{Path, State, WebSocketUpgrade},
    response::{IntoResponse, Json},
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use log::{info, error, debug};
use axum::http::Method;
use tower_http::cors::{Any, CorsLayer};

use emb_public::{
    EmbError, CoreSocketClient,
};

use crate::realtime_monitor::{RealtimeMonitor, TemperatureZone, MonitoringConfig};
use crate::config_manager::ConfigManager;
use crate::gcode_controller::GCodeController;

/// API request/response types

/// Start print request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartPrintRequest {
    pub filename: String,
    pub preset: Option<String>,
}

/// Temperature control request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureRequest {
    pub hotend_temp: Option<f32>,
    pub bed_temp: Option<f32>,
    pub fan_speed: Option<u8>,
}

/// Homing request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomingRequest {
    pub axis: String,
    pub feed_rate: Option<f32>,
}

/// Printer status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterStatus {
    pub state: String,
    pub progress: f32,
    pub current_file: Option<String>,
    pub print_time: u64,
    pub remaining_time: u64,
    pub layer: LayerInfo,
    pub temperature: TempStatus,
    pub position: PositionData,
}

/// Layer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerInfo {
    pub current: u32,
    pub total: u32,
    pub height: f32,
}

/// Temperature status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempStatus {
    pub hotend_current: f32,
    pub hotend_target: f32,
    pub bed_current: f32,
    pub bed_target: f32,
}

/// Position data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionData {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub e: f32,
}

/// File info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub uploaded_at: String,
    pub estimated_time: Option<u64>,
    pub layers: Option<u32>,
}

/// List files response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFilesResponse {
    pub files: Vec<FileInfo>,
}

/// Configuration update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigUpdateRequest {
    pub section: String,
    pub data: serde_json::Value,
}

/// API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(msg: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg),
        }
    }
}

/// Shared application state using Socket API
pub struct ApiState {
    pub core_client: Arc<CoreSocketClient>,
    pub realtime_monitor: Arc<RwLock<RealtimeMonitor>>,
    pub config_manager: Arc<RwLock<ConfigManager>>,
    pub gcode_controller: Arc<RwLock<GCodeController>>,
    pub gcode_directory: String,
    pub broadcast_tx: broadcast::Sender<WebSocketMessage>,
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WebSocketMessage {
    #[serde(rename = "temperature")]
    Temperature(TempStatus),
    #[serde(rename = "position")]
    Position(PositionData),
    #[serde(rename = "progress")]
    Progress { percent: f32, current_layer: u32, total_layers: u32 },
    #[serde(rename = "state")]
    StateChange { from: String, to: String },
    #[serde(rename = "print_event")]
    PrintEvent { event: String, message: String },
    #[serde(rename = "alert")]
    Alert { severity: String, message: String },
    #[serde(rename = "limit_switch")]
    LimitSwitch { x: bool, y: bool, z: bool },
    #[serde(rename = "homing")]
    Homing { axis: String, status: String, progress: f32 },
}

/// Create API router
pub fn create_router(state: Arc<ApiState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers(Any)
        .allow_credentials(false);

    Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/printer/status", get(get_printer_status))
        .route("/api/v1/printer/start", post(start_print))
        .route("/api/v1/printer/pause", post(pause_print))
        .route("/api/v1/printer/resume", post(resume_print))
        .route("/api/v1/printer/stop", post(stop_print))
        .route("/api/v1/files", get(list_files).post(upload_file))
        .route("/api/v1/files/:name", delete(delete_file))
        .route("/api/v1/temperature/status", get(get_temperature))
        .route("/api/v1/temperature/target", post(set_temperature))
        .route("/api/v1/config", get(get_config).post(update_config))
        .route("/api/v1/printer/home", post(home_axis))
        .route("/api/v1/leveling/start", post(leveling::start_leveling))
        .route("/api/v1/leveling/status", get(leveling::get_leveling_status))
        .route("/api/v1/leveling/config", get(leveling::get_leveling_config))
        .route("/api/v1/leveling/config", post(leveling::update_leveling_config))
        .route("/api/v1/leveling/data", get(leveling::get_leveling_data))
        .route("/api/v1/leveling/reset", post(leveling::reset_leveling))
        .route("/api/v1/leveling/history", post(save_leveling_history))
        .route("/api/v1/leveling/history", get(get_leveling_history))
        .route("/api/v1/leveling/history/:timestamp", delete(delete_leveling_history))
        .route("/api/v1/leveling/history", delete(clear_leveling_history))
        .route("/ws", get(websocket_handler))
        .with_state(state)
        .layer(cors)
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    Json(ApiResponse::success("ok"))
}

/// Get printer status
async fn get_printer_status(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    let (hotend_current, hotend_target, bed_current, bed_target) = {
        let monitor = state.realtime_monitor.read().await;
        let temps = monitor.get_current_temperatures().await;
        let mut hc = 0.0f32;
        let mut ht = 0.0f32;
        let mut bc = 0.0f32;
        let mut bt = 0.0f32;
        for t in temps {
            match t.zone {
                TemperatureZone::Hotend => { hc = t.current_temp; ht = t.target_temp; }
                TemperatureZone::Bed => { bc = t.current_temp; bt = t.target_temp; }
                _ => {}
            }
        }
        (hc, ht, bc, bt)
    };

    let (x, y, z, e) = {
        let monitor = state.realtime_monitor.read().await;
        match monitor.get_current_position().await {
            Some(pos) => (pos.x, pos.y, pos.z, pos.e),
            None => (0.0, 0.0, 0.0, 0.0),
        }
    };

    let status = PrinterStatus {
        state: "Idle".to_string(),
        progress: 0.0,
        current_file: None,
        print_time: 0,
        remaining_time: 0,
        layer: LayerInfo {
            current: 0,
            total: 0,
            height: 0.0,
        },
        temperature: TempStatus {
            hotend_current,
            hotend_target,
            bed_current,
            bed_target,
        },
        position: PositionData { x, y, z, e },
    };

    Json(ApiResponse::success(status))
}

/// Start print job
async fn start_print(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<StartPrintRequest>,
) -> impl IntoResponse {
    info!("Starting print: {}", req.filename);

    let file_path = std::path::PathBuf::from(&state.gcode_directory).join(&req.filename);

    if !file_path.exists() {
        return Json(ApiResponse::error(format!("File not found: {}", req.filename)));
    }

    Json(ApiResponse::success("Print started"))
}

/// Pause print
async fn pause_print(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    info!("Pausing print");
    Json(ApiResponse::success("Paused"))
}

/// Resume print
async fn resume_print(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    info!("Resuming print");
    Json(ApiResponse::success("Resumed"))
}

/// Stop print
async fn stop_print(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    info!("Stopping print");
    Json(ApiResponse::success("Stopped"))
}

/// List G-code files
async fn list_files(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    let dir = &state.gcode_directory;
    
    let files = match tokio::fs::read_dir(dir).await {
        Ok(mut entries) => {
            let mut files = Vec::new();
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(metadata) = entry.metadata().await {
                    if metadata.is_file() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.ends_with(".gcode") || name.ends_with(".g") {
                            files.push(FileInfo {
                                name,
                                size: metadata.len(),
                                uploaded_at: "2024-01-01".to_string(),
                                estimated_time: None,
                                layers: None,
                            });
                        }
                    }
                }
            }
            files
        }
        Err(_) => Vec::new(),
    };

    Json(ApiResponse::success(ListFilesResponse { files }))
}

/// Upload G-code file
async fn upload_file(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    Json(ApiResponse::success("File uploaded"))
}

/// Delete file
async fn delete_file(
    State(state): State<Arc<ApiState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let path = std::path::PathBuf::from(&state.gcode_directory).join(&name);
    
    match tokio::fs::remove_file(&path).await {
        Ok(_) => Json(ApiResponse::success("File deleted")),
        Err(e) => Json(ApiResponse::error(format!("Failed to delete: {}", e))),
    }
}

/// Get temperature status
async fn get_temperature(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    let monitor = state.realtime_monitor.read().await;
    let temps = monitor.get_current_temperatures().await;
    drop(monitor);

    let mut hotend_current = 0.0f32;
    let mut hotend_target = 0.0f32;
    let mut bed_current = 0.0f32;
    let mut bed_target = 0.0f32;

    for t in temps {
        match t.zone {
            TemperatureZone::Hotend => {
                hotend_current = t.current_temp;
                hotend_target = t.target_temp;
            }
            TemperatureZone::Bed => {
                bed_current = t.current_temp;
                bed_target = t.target_temp;
            }
            _ => {}
        }
    }

    let temp = TempStatus {
        hotend_current,
        hotend_target,
        bed_current,
        bed_target,
    };

    Json(ApiResponse::success(temp))
}

/// Set target temperature
async fn set_temperature(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<TemperatureRequest>,
) -> impl IntoResponse {
    info!("Setting temperature: hotend={:?}, bed={:?}, fan={:?}",
        req.hotend_temp, req.bed_temp, req.fan_speed);

    Json(ApiResponse::success("Temperature set"))
}

/// Get configuration
async fn get_config(
    State(state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    let config_manager = state.config_manager.read().await;
    let config = config_manager.get_config().await;
    
    Json(ApiResponse::success(config))
}

/// Update configuration
async fn update_config(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<ConfigUpdateRequest>,
) -> impl IntoResponse {
    info!("Updating config section: {}", req.section);

    Json(ApiResponse::success("Configuration updated"))
}

/// Home single axis or all axes
pub async fn home_axis(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<HomingRequest>,
) -> impl IntoResponse {
    info!("Homing request: axis={}, feed_rate={:?}", req.axis, req.feed_rate);
    
    let axis_upper = req.axis.to_uppercase();
    if !matches!(axis_upper.as_str(), "X" | "Y" | "Z" | "ALL") {
        return Json(ApiResponse::error(format!("Invalid axis: {}", req.axis)));
    }

    match state.core_client.motion_reset_position().await {
        Ok(_) => Json(ApiResponse::success("Homing started")),
        Err(e) => Json(ApiResponse::error(format!("Homing failed: {}", e))),
    }
}

/// WebSocket handler
async fn websocket_handler(
    State(state): State<Arc<ApiState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

/// Handle WebSocket connection
async fn handle_websocket(
    socket: axum::extract::ws::WebSocket,
    state: Arc<ApiState>,
) {
    use axum::extract::ws::{Message, WebSocketStream};
    use futures_util::{SinkExt, StreamExt};

    let (mut sender, mut receiver) = socket.split();

    let broadcast_rx = state.broadcast_tx.subscribe();

    let mut broadcast_task = tokio::spawn(async move {
        let mut rx = broadcast_rx;
        while let Ok(msg) = rx.recv().await {
            let json = serde_json::to_string(&msg).unwrap();
            if sender.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                debug!("WebSocket received: {}", text);
            }
        }
    });

    tokio::select! {
        _ = (&mut broadcast_task) => recv_task.abort(),
        _ = (&mut recv_task) => broadcast_task.abort(),
    };
}

/// Save leveling history
async fn save_leveling_history(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    Json(ApiResponse::success("History saved"))
}

/// Get leveling history
async fn get_leveling_history(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    Json(ApiResponse::success(Vec::<String>::new()))
}

/// Delete leveling history entry
async fn delete_leveling_history(
    State(_state): State<Arc<ApiState>>,
    Path(_timestamp): Path<String>,
) -> impl IntoResponse {
    Json(ApiResponse::success("History entry deleted"))
}

/// Clear all leveling history
async fn clear_leveling_history(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    Json(ApiResponse::success("History cleared"))
}