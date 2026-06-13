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
    response::{Html, IntoResponse, Json, sse::{Event, Sse}},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};
use axum::http::Method;
use tokio::sync::broadcast;

use emb_public::{CoreSocketClient, ConfigManager};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GpioReportEvent {
    name: String,
    value: f32,
    /// 事件动作（如 "filament_runout", "power_loss"），无事件时为 None
    action: Option<String>,
}

struct DebugState {
    core_client: Arc<CoreSocketClient>,
    /// GPIO Report事件广播通道
    gpio_report_tx: broadcast::Sender<GpioReportEvent>,
}

fn create_debug_router(state: Arc<DebugState>) -> Router {
    Router::new()
        .route("/", get(debug_page))
        .route("/api/status", get(get_status))
        .route("/api/config/load", post(load_configs))
        .route("/api/config/reload", post(config_reload))  // 新增：重新加载配置（自动发送Mesh数据）
        .route("/api/serial/connect", post(serial_connect))
        .route("/api/gpio/set", get(gpio_set))
        .route("/api/gpio/query", get(gpio_query))
        .route("/api/gpio/report/stream", get(gpio_report_stream))
        .route("/api/homing/start", get(homing_start))
        .route("/api/motion/gcode", post(gcode_execute))  // 新增：执行G指令
        .route("/api/mesh/clear", post(mesh_clear))  // 新增：手动清除Mesh数据
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
    
    log::info!("Loading and sending configs from: {}", config_dir);
    
    // Load config files locally and send directly to server (new way)
    match ConfigManager::instance().load(&config_dir) {
        Ok(()) => {
            log::info!("✅ Configs loaded from {}", config_dir);
            match ConfigManager::instance().reload(&state.core_client).await {
                Ok(()) => {
                    log::info!("✅ Configs sent to server successfully");
                    Json(ApiResponse::success("Configuration loaded and sent successfully".to_string()))
                }
                Err(e) => Json(ApiResponse::<String>::error(format!("Failed to send configs: {}", e))),
            }
        }
        Err(e) => Json(ApiResponse::<String>::error(format!("Failed to load configs: {}", e))),
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
            
            // 串口连接成功后订阅GPIO Report
            subscribe_gpio_report(&state.core_client).await;
            
            // Wait for server to send GPIO config and ConfigComplete first
            log::info!("Waiting for server to send GPIO config...");
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            
            // Load configs first
            let config_dir = std::env::current_dir()
                .map(|p| p.join("config"))
                .unwrap_or_else(|_| std::path::PathBuf::from("config"));
            
            match ConfigManager::instance().load(&config_dir.to_string_lossy()) {
                Ok(()) => {
                    log::info!("✅ Configs loaded from {}", config_dir.display());
                    
                    // Send all configs to server and device (including Mesh data)
                    log::info!("Sending all configs to server and device...");
                    match ConfigManager::instance().reload(&state.core_client).await {
                        Ok(()) => log::info!("✅ All configs sent (including Mesh data)"),
                        Err(e) => log::warn!("Failed to send configs: {}", e),
                    }
                }
                Err(e) => log::warn!("Failed to load configs: {}", e),
            }
            
            Json(ApiResponse::success(StatusInfo {
                serial_connected: true,
                serial_port: Some(req.port),
            }))
        }
        Err(e) => Json(ApiResponse::<StatusInfo>::error(format!("Connect failed: {}", e))),
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

/// 订阅GPIO Report（仅在串口连接后调用）
async fn subscribe_gpio_report(core_client: &CoreSocketClient) {
    match core_client.gpio_subscribe_report(true).await {
        Ok(()) => log::info!("Subscribed to GPIO Report"),
        Err(e) => log::warn!("Failed to subscribe GPIO Report: {}", e),
    }
}

/// GPIO Report SSE流
async fn gpio_report_stream(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    use std::convert::Infallible;
    use tokio_stream::StreamExt;
    
    let rx = state.gpio_report_tx.subscribe();
    
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .filter_map(|result| {
            match result {
                Ok(event) => {
                    let json = serde_json::to_string(&event).ok()?;
                    Some(Ok::<Event, Infallible>(Event::default().data(json)))
                }
                Err(_) => None,
            }
        });
    
    Sse::new(stream)
}

/// Homing Start: send 0x0A frame with axes_mask
/// Query params: ?x=true&y=true&z=true  or  ?all=true
#[derive(Debug, Deserialize)]
struct HomingStartRequest {
    #[serde(default)]
    x: bool,
    #[serde(default)]
    y: bool,
    #[serde(default)]
    z: bool,
    /// Shortcut: home all axes
    #[serde(default)]
    all: bool,
}

async fn homing_start(
    State(state): State<Arc<DebugState>>,
    axum::extract::Query(req): axum::extract::Query<HomingStartRequest>,
) -> impl IntoResponse {
    let axes_mask: u8 = if req.all {
        0b111
    } else {
        (if req.x { 0x01 } else { 0 }) |
        (if req.y { 0x02 } else { 0 }) |
        (if req.z { 0x04 } else { 0 })
    };

    if axes_mask == 0 {
        return Json(ApiResponse::<String>::error("No axes selected".to_string()));
    }

    let axes_names = [
        (if axes_mask & 0x01 != 0 { "X" } else { "" }),
        (if axes_mask & 0x02 != 0 { "Y" } else { "" }),
        (if axes_mask & 0x04 != 0 { "Z" } else { "" }),
    ].concat();

    log::info!("Homing start: axes_mask=0x{:02X} ({})", axes_mask, axes_names);

    match state.core_client.serial_send_frame(0x0A, vec![axes_mask]).await {
        Ok(_) => {
            // Poll for response (ACK/NACK) from device
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            for _ in 0..5 {
                match state.core_client.serial_recv_frame().await {
                    Ok(Some((ft, pld))) => {
                        if ft == 0x12 {
                            // NACK
                            let err = if pld.len() > 1 { pld[1] } else { 0xFF };
                            log::error!("Homing NACK (error={}, meaning follows):", err);
                            let desc = match err {
                                1 => "BUSY - homing already running",
                                2 => "INVALID_AXES",
                                3 => "NOT_CFG - endstops not configured",
                                4 => "PRE_TIMEOUT",
                                5 => "TOTAL_TIMEOUT",
                                6 => "AXIS_DISABLED",
                                _ => "UNKNOWN",
                            };
                            log::error!("  -> {}", desc);
                        } else if ft == 0x06 {
                        }
                    }
                    Ok(None) => break, // no more frames
                    Err(e) => log::warn!("Recv error: {}", e),
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Json(ApiResponse::success(format!("Homing started: {}", axes_names)))
        }
        Err(e) => Json(ApiResponse::<String>::error(format!("Failed: {}", e))),
    }
}

/// Config Reload: 重新加载配置并发送到服务端
/// 调用ConfigManager::reload()，自动发送：
/// - Motion config
/// - Fan config
/// - Mesh数据（如果存在）
/// - Hardware config到下位机
/// - ConfigComplete到下位机
async fn config_reload(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    log::info!("Config Reload: Reloading configuration and sending to server...");
    
    // 调用ConfigManager::reload()（会自动发送Mesh数据）
    match ConfigManager::instance().reload(&state.core_client).await {
        Ok(()) => {
            log::info!("✅ Configuration reloaded successfully (Mesh data sent if available)");
            Json(ApiResponse::success("Configuration reloaded successfully".to_string()))
        }
        Err(e) => {
            log::error!("❌ Config reload failed: {}", e);
            Json(ApiResponse::<String>::error(format!("Config reload failed: {}", e)))
        }
    }
}

/// Mesh Clear: 清除服务端的Mesh数据
async fn mesh_clear(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    log::info!("Mesh Clear: Clearing mesh data from server...");
    
    use emb_api::{CoreRequest, MotionRequest};
    
    let clear_request = CoreRequest::Motion(MotionRequest::ClearMesh);
    
    match state.core_client.send_request(&clear_request).await {
        Ok(response) => {
            log::info!("✅ Mesh cleared successfully");
            Json(ApiResponse::success(format!("Mesh cleared: {:?}", response)))
        }
        Err(e) => {
            log::error!("❌ Mesh clear failed: {}", e);
            Json(ApiResponse::<String>::error(format!("Mesh clear failed: {}", e)))
        }
    }
}

/// G-code Execute: 执行G指令（比如G1 X100 Y100 Z0.2）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GcodeRequest {
    gcode: String,
}

async fn gcode_execute(
    State(state): State<Arc<DebugState>>,
    Json(req): Json<GcodeRequest>,
) -> impl IntoResponse {
    log::info!("G-code Execute: {}", req.gcode);
    
    // Parse G-code (simple parser for G0/G1)
    let parts: Vec<&str> = req.gcode.split_whitespace().collect();
    if parts.is_empty() {
        return Json(ApiResponse::<String>::error("Empty G-code".to_string()));
    }
    
    let cmd = parts[0].to_uppercase();
    if cmd != "G0" && cmd != "G1" {
        return Json(ApiResponse::<String>::error(format!("Unsupported G-code: {}", cmd)));
    }
    
    // Parse parameters (X, Y, Z, E, F)
    let mut x: Option<f32> = None;
    let mut y: Option<f32> = None;
    let mut z: Option<f32> = None;
    let mut e: Option<f32> = None;
    let mut feed_rate: Option<f32> = None;
    
    for part in parts.iter().skip(1) {
        let part = part.to_uppercase();
        if part.starts_with('X') {
            x = part[1..].parse().ok();
        } else if part.starts_with('Y') {
            y = part[1..].parse().ok();
        } else if part.starts_with('Z') {
            z = part[1..].parse().ok();
        } else if part.starts_with('E') {
            e = part[1..].parse().ok();
        } else if part.starts_with('F') {
            feed_rate = part[1..].parse().ok();
        }
    }
    
    // Dispatch motion to server
    match state.core_client.motion_dispatch(&cmd, x, y, z, e, feed_rate).await {
        Ok(_) => {
            Json(ApiResponse::success("Motion dispatched".to_string()))
        }
        Err(e) => {
            log::error!("❌ Motion dispatch failed: {}", e);
            Json(ApiResponse::<String>::error(format!("Motion dispatch failed: {}", e)))
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        "info,emb_public::config::config_protocol=debug,debug_terminal=debug"
    )).init();

    let args: Vec<String> = std::env::args().collect();
    let http_addr = args.get(1).unwrap_or(&"127.0.0.1:8080".to_string()).clone();
    let core_addr = args.get(2).unwrap_or(&"127.0.0.1:9527".to_string()).clone();
    let serial_port = args.get(3).cloned();
    let baud_rate: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(57600);

    log::info!("Debug Terminal starting...");
    log::info!("HTTP server: {}", http_addr);
    log::info!("Core server: {}", core_addr);

    let core_client = Arc::new(CoreSocketClient::default_client(&core_addr));
    
    // 创建GPIO Report广播通道
    let (gpio_report_tx, _) = broadcast::channel(16);
    
    match core_client.connect().await {
        Ok(()) => log::info!("Connected to core server"),
        Err(e) => {
            log::error!("Failed to connect to core server: {}", e);
            log::error!("Make sure emb-core-server is running at {}", core_addr);
            std::process::exit(1);
        }
    }
    
    // 设置GPIO Report回调（提前设置，实际订阅在串口连接后）
    {
        let tx = gpio_report_tx.clone();
        core_client.set_gpio_report_callback(move |name, value| {
            log::info!("GPIO Report: {} = {}", name, value);

            // 推送到SSE流（不包含事件信息，客户端自行处理）
            let _ = tx.send(GpioReportEvent { name, value, action: None });
        }).await;
    }
    
    // 注意：GPIO订阅需要在串口连接之后才能成功
    // 将在 serial_connect 处理函数中订阅

    if let Some(port) = serial_port {
        log::info!("Auto-connecting serial: {} @ {}", port, baud_rate);
        match core_client.serial_connect(&port, baud_rate).await {
            Ok(()) => {
                log::info!("Serial connected to {}", port);
                // 串口连接成功后订阅GPIO Report
                subscribe_gpio_report(&core_client).await;
            }
            Err(e) => log::error!("Serial connect failed: {}", e),
        }
    }

    // 后台 pinger：每2秒 ping 一次，触发 read_response 消费 GPIO Report 推送
    let ping_client = core_client.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = ping_client.ping().await;
        }
    });

    let state = Arc::new(DebugState { 
        core_client,
        gpio_report_tx,
    });

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
