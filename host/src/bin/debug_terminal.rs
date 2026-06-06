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
        .route("/api/serial/connect", post(serial_connect))
        .route("/api/gpio/set", get(gpio_set))
        .route("/api/gpio/query", get(gpio_query))
        .route("/api/gpio/report/stream", get(gpio_report_stream))
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
            
            // 串口连接成功后订阅GPIO Report
            subscribe_gpio_report(&state.core_client).await;
            
            // Wait for server to send GPIO config and ConfigComplete first
            log::info!("Waiting for server to send GPIO config...");
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            
            let config_dir = std::env::current_dir()
                .map(|p| p.join("config"))
                .unwrap_or_else(|_| std::path::PathBuf::from("config"));
            
            match config_adapter::load_configs(&config_dir.to_string_lossy()) {
                Ok(configs) => {
                    log::info!("Configs loaded successfully");
                    
                    let printer_config = config_adapter::build_printer_config(&configs);
                    let config_frames = ConfigFrameBuilder::build_config_frames(&printer_config);
                    
                    if config_frames.is_empty() {
                        log::info!("No config frames to send (GPIO config sent by server)");
                    } else {
                        log::info!("Sending {} config frames to device...", config_frames.len());
                        
                        for frame_bytes in &config_frames {
                            match state.core_client.serial_send_raw(frame_bytes).await {
                                Ok(()) => log::debug!("Config frame sent: {} bytes", frame_bytes.len()),
                                Err(e) => log::warn!("Failed to send config frame: {}", e),
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                        
                        log::info!("All config frames sent");
                    }
                    
                    // Wait for server's ConfigComplete to be processed
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    
                    match state.core_client.serial_init_seq().await {
                        Ok(()) => log::info!("Device seq initialized"),
                        Err(e) => log::warn!("Init seq failed: {}", e),
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
