//! Debug Terminal Server
//!
//! Simple HTTP server for GPIO debugging and interactive testing.
//! Initializes automatically following the standard setup flow:
//!   load configs → connect core server → init device (serial + GPIO subscribe + configs + seq)
//!
//! Usage: debug_terminal [http_addr] [core_addr]
//!
//! Example:
//!   debug_terminal 127.0.0.1:8080 127.0.0.1:9527
//!
//! Then open http://127.0.0.1:8080/debug in browser.

use std::sync::Arc;
use host::setup;
use std::sync::atomic::{AtomicBool, Ordering};
use axum::{
    extract::State,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};
use axum::http::Method;

use emb_public::{CoreSocketClient, TemperatureManager, TemperatureManagerConfig, SyncEventPublisher, GpioManager, HomingManager, PrintController, ConfigManager};
use emb_public::gcode::MotionCommand;

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
struct GpioInfo {
    name: String,
    value: f32,
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

/// Gcode print request
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GcodePrintRequest {
    filename: String,
    #[serde(default)]
    resume: bool,
}

/// Temperature status response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TemperatureStatus {
    hotend_current: f32,
    hotend_target: f32,
    bed_current: f32,
    bed_target: f32,
}

/// Temperature set request
#[derive(Debug, Deserialize)]
struct TemperatureSetRequest {
    heater: String,
    temp: f32,
}

struct DebugState {
    core_client: Arc<CoreSocketClient>,
    /// GPIO Manager (handles pin control + event broadcast)
    gpio_manager: GpioManager,
    /// Temperature manager
    temperature_manager: Arc<TemperatureManager>,
    /// Homing Manager (encapsulates homing protocol)
    homing_manager: HomingManager,
    /// Print Controller (manages print state + job execution)
    print_controller: Arc<tokio::sync::RwLock<PrintController>>,
    /// Gcodes directory
    gcodes_dir: String,
    /// Whether a print is currently running
    print_running: Arc<AtomicBool>,
}

fn create_debug_router(state: Arc<DebugState>) -> Router {
    Router::new()
        .route("/", get(debug_page))
        .route("/api/gpio/set", get(gpio_set))
        .route("/api/gpio/query", get(gpio_query))
        .route("/api/homing/start", get(homing_start))
        .route("/api/motion/gcode", post(gcode_execute))
        .route("/api/gcode/files", get(gcode_file_list))
        .route("/api/gcode/print", post(gcode_file_print))
        .route("/api/gcode/progress", get(gcode_print_progress))
        .route("/api/temperature/status", get(temperature_status))
        .route("/api/temperature/set", get(temperature_set))
        .route("/api/motion/position", get(motion_position))
        .route("/api/checkpoint/query", get(checkpoint_query))
        .route("/api/checkpoint/clear", get(checkpoint_clear))
        .with_state(state)
}

async fn debug_page() -> impl IntoResponse {
    Html(include_str!("../debug_terminal/debug.html"))
}

async fn gpio_set(
    State(state): State<Arc<DebugState>>,
    axum::extract::Query(req): axum::extract::Query<GpioSetRequest>,
) -> impl IntoResponse {
    tracing::info!("Debug: GPIO set {} = {}", req.name, req.value);
    
    match state.gpio_manager.set_pin(&req.name, req.value).await {
        Ok(_) => Json(ApiResponse::success(GpioInfo { name: req.name, value: req.value })),
        Err(e) => Json(ApiResponse::<GpioInfo>::error(format!("Error: {}", e))),
    }
}

async fn gpio_query(
    State(state): State<Arc<DebugState>>,
    axum::extract::Query(req): axum::extract::Query<GpioQueryRequest>,
) -> impl IntoResponse {
    tracing::info!("Debug: GPIO query {}", req.name);
    
    match state.gpio_manager.query_pin(&req.name).await {
        Ok(value) => Json(ApiResponse::success(GpioInfo { name: req.name, value })),
        Err(e) => Json(ApiResponse::<GpioInfo>::error(format!("Error: {}", e))),
    }
}

/// Temperature status: return current and target temps for all heaters
async fn temperature_status(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    let status = state.temperature_manager.get_temp_status().await;
    Json(ApiResponse::success(TemperatureStatus {
        hotend_current: status.hotend_current,
        hotend_target: status.hotend_target,
        bed_current: status.bed_current,
        bed_target: status.bed_target,
    }))
}

/// Temperature set: set target temperature for a heater
/// Query params: ?heater=hotend&temp=200
async fn temperature_set(
    State(state): State<Arc<DebugState>>,
    axum::extract::Query(req): axum::extract::Query<TemperatureSetRequest>,
) -> impl IntoResponse {
    tracing::info!("Temperature set: {} = {}°C", req.heater, req.temp);
    match state.temperature_manager.set_target(&req.heater, req.temp).await {
        Ok(_) => Json(ApiResponse::success(format!("{} target set to {}°C", req.heater, req.temp))),
        Err(e) => Json(ApiResponse::<String>::error(format!("Error: {}", e))),
    }
}

#[derive(Debug, Serialize)]
struct PositionInfo {
    x: f32,
    y: f32,
    z: f32,
    e: f32,
}

/// Get current position from server planner (coordinate system)
async fn motion_position(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    match state.core_client.motion_get_position().await {
        Ok((x, y, z, e)) => Json(ApiResponse::success(PositionInfo { x, y, z, e })),
        Err(e) => Json(ApiResponse::<PositionInfo>::error(format!("Error: {}", e))),
    }
}

/// Homing Start: use HomingManager
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
    match state.homing_manager.home_by_names(req.x, req.y, req.z, req.all).await {
        Ok(msg) => Json(ApiResponse::success(msg)),
        Err(e) => Json(ApiResponse::<String>::error(e)),
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
    tracing::info!("G-code Execute: {}", req.gcode);
    
    // Parse G-code using the public parser
    let parsed = emb_public::gcode::GCodeParser::parse_line(&req.gcode, 0);
    
    let parsed = match parsed {
        Some(p) => p,
        None => {
            return Json(ApiResponse::<String>::error("Parse error".to_string()));
        }
    };
    
    match parsed.kind {
        emb_public::gcode::CommandKind::Motion(motion_cmd) => {
            match motion_cmd {
                MotionCommand::LinearMove { x, y, z, e, f, is_rapid } => {
                    let cmd = if is_rapid { "G0" } else { "G1" };
                    
                    // G0/G1 homed check before dispatch
                    match state.core_client.motion_query_homed().await {
                        Ok(homed) if homed != 0b111 => {
                            return Json(ApiResponse::<String>::error(
                                format!("Cannot move: axes not homed (homed_axes={:#04b}). Use G28 first.", homed)
                            ));
                        }
                        Err(e) => {
                            return Json(ApiResponse::<String>::error(format!("Homed query failed: {}", e)));
                        }
                        _ => {}
                    }
                    
                    match state.core_client.motion_dispatch(cmd, x, y, z, e, f).await {
                        Ok(msg) => Json(ApiResponse::success(if msg.is_empty() { "Motion dispatched".to_string() } else { msg })),
                        Err(e) => Json(ApiResponse::<String>::error(format!("Motion dispatch failed: {}", e))),
                    }
                }
                MotionCommand::ArcMove { x, y, z, e, f, i, j, is_cw } => {
                    let cmd = if is_cw { "G2" } else { "G3" };
                    let arc = Some(emb_api::ArcParamsApi {
                        i, j,
                        direction: if is_cw { 0 } else { 1 },
                    });
                    
                    // Arc also requires homed axes
                    match state.core_client.motion_query_homed().await {
                        Ok(homed) if homed != 0b111 => {
                            return Json(ApiResponse::<String>::error(
                                format!("Cannot move: axes not homed (homed_axes={:#04b}). Use G28 first.", homed)
                            ));
                        }
                        Err(e) => {
                            return Json(ApiResponse::<String>::error(format!("Homed query failed: {}", e)));
                        }
                        _ => {}
                    }
                    
                    match state.core_client.motion_dispatch_arc(&cmd, x, y, z, e, f, arc).await {
                        Ok(msg) => Json(ApiResponse::success(if msg.is_empty() { "Arc dispatched".to_string() } else { msg })),
                        Err(e) => Json(ApiResponse::<String>::error(format!("Arc dispatch failed: {}", e))),
                    }
                }
                MotionCommand::Home { x, y, z } => {
                    // G28 - route through HomingManager
                    match state.homing_manager.home_by_names(x, y, z, false).await {
                        Ok(msg) => Json(ApiResponse::success(msg)),
                        Err(e) => Json(ApiResponse::<String>::error(e)),
                    }
                }
                MotionCommand::SetPosition { x, y, z, e } => {
                    // G92
                    match state.core_client.motion_set_position(x, y, z, e).await {
                        Ok(_) => Json(ApiResponse::success("Position set".to_string())),
                        Err(e) => Json(ApiResponse::<String>::error(e)),
                    }
                }
                MotionCommand::AbsolutePositioning => {
                    // G90 - send via MCommand
                    match state.core_client.motion_execute_m_command(emb_api::MCommand::AbsolutePositioning).await {
                        Ok(_) => Json(ApiResponse::success("Absolute positioning (G90)".to_string())),
                        Err(e) => Json(ApiResponse::<String>::error(e)),
                    }
                }
                MotionCommand::RelativePositioning => {
                    // G91
                    match state.core_client.motion_execute_m_command(emb_api::MCommand::RelativePositioning).await {
                        Ok(_) => Json(ApiResponse::success("Relative positioning (G91)".to_string())),
                        Err(e) => Json(ApiResponse::<String>::error(e)),
                    }
                }
                MotionCommand::Dwell { dwell_time_ms } => {
                    // G4 — motion_dwell 内部等待设备端 DwellDone 通知
                    match state.core_client.motion_dwell(dwell_time_ms).await {
                        Ok(_) => Json(ApiResponse::success(format!("Dwell {}ms", dwell_time_ms))),
                        Err(e) => Json(ApiResponse::<String>::error(e)),
                    }
                }
                _ => {
                    Json(ApiResponse::<String>::error(format!("Unsupported G-code: {}", req.gcode)))
                }
            }
        }
        emb_public::gcode::CommandKind::Machine(m_cmd) => {
            tracing::info!("  → Executing M-command: {:?}", m_cmd);

            // M119 - 限位状态查询，特殊处理以显示配置中定义的限位开关
            if let emb_api::MCommand::GetEndstopStates = &m_cmd {
                match state.core_client.motion_get_endstop_states().await {
                    Ok((x_min, x_max, y_min, y_max, z_min, z_max)) => {
                        // 读取配置文件，只显示已配置的限位开关
                        let config = ConfigManager::instance().get_config().ok();
                        let limit = config.as_ref().map(|c| &c.limit_switch);

                        let mut parts = Vec::new();
                        if let Some(l) = limit {
                            if !l.x.pin.is_empty() {
                                parts.push(format!("x_min:{}", if x_min { "TRIGGERED" } else { "open" }));
                            }
                            if !l.y.pin.is_empty() {
                                parts.push(format!("y_min:{}", if y_min { "TRIGGERED" } else { "open" }));
                            }
                            if !l.z.pin.is_empty() {
                                parts.push(format!("z_min:{}", if z_min { "TRIGGERED" } else { "open" }));
                            }
                        } else {
                            // 无配置时，显示所有限位状态
                            parts.push(format!("x_min:{}", if x_min { "TRIGGERED" } else { "open" }));
                            parts.push(format!("x_max:{}", if x_max { "TRIGGERED" } else { "open" }));
                            parts.push(format!("y_min:{}", if y_min { "TRIGGERED" } else { "open" }));
                            parts.push(format!("y_max:{}", if y_max { "TRIGGERED" } else { "open" }));
                            parts.push(format!("z_min:{}", if z_min { "TRIGGERED" } else { "open" }));
                            parts.push(format!("z_max:{}", if z_max { "TRIGGERED" } else { "open" }));
                        }

                        let msg = format!("Endstop states: {}", parts.join(" "));
                        Json(ApiResponse::success(msg))
                    }
                    Err(e) => Json(ApiResponse::<String>::error(format!("M119 failed: {}", e))),
                }
            } else {
                match state.core_client.motion_execute_m_command(m_cmd).await {
                    Ok(_) => Json(ApiResponse::success("M-command executed".to_string())),
                    Err(e) => Json(ApiResponse::<String>::error(format!("M-command failed: {}", e))),
                }
            }
        }
        _ => {
            Json(ApiResponse::<String>::error(format!("Unsupported command: {}", req.gcode)))
        }
    }
}

/// List .gcode files in the gcodes directory
async fn gcode_file_list(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    let dir = &state.gcodes_dir;
    let mut files = Vec::new();

    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".gcode") || name.ends_with(".gc") || name.ends_with(".g") {
                        files.push(name.to_string());
                    }
                }
            }
            files.sort();
            Json(ApiResponse::success(files))
        }
        Err(e) => {
            tracing::warn!("Failed to read gcodes dir '{}': {}", dir, e);
            Json(ApiResponse::<Vec<String>>::error(format!("Cannot read directory: {}", e)))
        }
    }
}

/// Start printing a gcode file using PrintController.execute_print_loop()
async fn gcode_file_print(
    State(state): State<Arc<DebugState>>,
    Json(req): Json<GcodePrintRequest>,
) -> impl IntoResponse {
    // Check if already printing
    if state.print_running.load(Ordering::SeqCst) {
        return Json(ApiResponse::<String>::error("A print is already running".to_string()));
    }

    let file_path = format!("{}/{}", state.gcodes_dir, req.filename);
    tracing::info!("Starting print from file: {}", file_path);

    // Quick file existence check without loading into memory.
    // The file is read lazily inside execute_print_loop via BufReader.
    if !std::path::Path::new(&file_path).exists() {
        return Json(ApiResponse::<String>::error(format!("File not found: {}", file_path)));
    }

    // Set running flag
    state.print_running.store(true, Ordering::SeqCst);

    // Clone state for background task
    let state_clone = state.clone();
    let filename = req.filename.clone();

    // Spawn background print task using PrintController.
    // execute_print_loop handles all file I/O internally (BufReader, two-pass scan).
    let resume = req.resume;
    tokio::spawn(async move {
        let print_controller = state_clone.print_controller.read().await;
        let result = print_controller.execute_print_loop(
            &filename,
            &file_path,
            resume,
        ).await;

        let success = result.is_ok();
        if !success {
            tracing::error!("Print {} failed: {:?}", filename, result.err());
        }

        state_clone.print_running.store(false, Ordering::SeqCst);
        tracing::info!("Print {}: {}", filename, if success { "completed" } else { "failed" });
    });

    Json(ApiResponse::success(format!("Print started: {}", req.filename)))
}

/// Poll print progress from PrintController
async fn gcode_print_progress(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    let print_controller = state.print_controller.read().await;
    let progress = print_controller.get_progress().await;

    #[derive(Serialize)]
    struct PrintProgressResponse {
        filename: String,
        current_line: u32,
        total_lines: u32,
        percent: f32,
        status: String,
        finished: bool,
        success: bool,
    }

    let running = state.print_running.load(Ordering::SeqCst);
    let resp = PrintProgressResponse {
        filename: String::new(),
        current_line: progress.current_line,
        total_lines: progress.total_lines,
        percent: progress.percent,
        status: if !running && progress.percent >= 100.0 {
            "Completed".to_string()
        } else if !running && progress.percent == 0.0 {
            "No print active".to_string()
        } else {
            progress.status.clone()
        },
        finished: !running,
        success: !running && progress.percent >= 100.0,
    };

    Json(ApiResponse::success(resp))
}

/// Query checkpoint status
async fn checkpoint_query(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    let print_controller = state.print_controller.read().await;
    let info = print_controller.checkpoint_info().unwrap_or_default();
    Json(ApiResponse::success(info))
}

/// Clear checkpoint (discard saved state)
async fn checkpoint_clear(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    let print_controller = state.print_controller.read().await;
    match print_controller.clear_checkpoint() {
        Ok(()) => Json(ApiResponse::<String>::success("Checkpoint cleared".to_string())),
        Err(e) => Json(ApiResponse::<String>::error(format!("Failed to clear checkpoint: {}", e))),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing from logging.json config
    let log_config = emb_public::config::log_config::LogConfig::load("config")
        .unwrap_or_default();
    emb_public::logger::init_tracing(&log_config)?;

    let args: Vec<String> = std::env::args().collect();
    let http_addr = args.get(1).unwrap_or(&"127.0.0.1:8080".to_string()).clone();
    let core_addr = args.get(2).unwrap_or(&"127.0.0.1:9527".to_string()).clone();

    tracing::info!("Debug Terminal starting...");
    tracing::info!("HTTP server: {}", http_addr);
    tracing::info!("Core server: {}", core_addr);

    // Step 1: Load all configuration files at once
    setup::load_all_configs(setup::CONFIG_DIR)?;

    // Step 2: Create host and connect to emb-core-server
    let host = setup::create_and_connect_host(&core_addr).await?;

    // Create GPIO Manager (internal broadcast channel)
    let gpio_manager = GpioManager::new(host.client());
    gpio_manager.setup_callback().await;

    // Step 3: Initialize device (serial, GPIO subscribe, configs, STM32 seq)
    setup::initialize_device(&host).await?;

    // Step 4: Create and initialize temperature manager
    let event_publisher = Arc::new(SyncEventPublisher::new());
    let temperature_manager = Arc::new(TemperatureManager::new(
        host.client(),
        event_publisher.clone(),
        TemperatureManagerConfig::default(),
        None,
    ));
    setup::initialize_temperature_manager(&temperature_manager).await?;

    // Create HomingManager
    let homing_manager = HomingManager::new(host.client());

    // Create PrintController with required dependencies
    let mut print_controller = PrintController::new();
    print_controller.set_client(host.client());
    print_controller.set_temperature_manager(temperature_manager.clone());
    print_controller.set_resume_path("config/resume.json");
    let print_controller = Arc::new(tokio::sync::RwLock::new(print_controller));

    // Background pinger: every 2s, triggers read_response to consume GPIO Report push
    let ping_client = host.client();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = ping_client.ping().await;
        }
    });

    // Background safety check loop — moved to SafetyMonitor (emb_public::safety::SafetyMonitor)
    // In production, SafetyMonitor is started by SafetyController.
    // let tm_safety = temperature_manager.clone();
    // tokio::spawn(async move {
    //     tm_safety.start_safety_check_loop().await;
    // });

    // Gcodes directory (default: ./gcodes relative to current working dir)
    let gcodes_dir = std::env::current_dir()
        .map(|p| p.join("gcodes").to_string_lossy().to_string())
        .unwrap_or_else(|_| "gcodes".to_string());

    let print_running = Arc::new(AtomicBool::new(false));

    let state = Arc::new(DebugState { 
        core_client: host.client(),
        gpio_manager,
        temperature_manager,
        homing_manager,
        print_controller,
        gcodes_dir,
        print_running,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    let app = Router::new()
        .nest("/debug", create_debug_router(state))
        .layer(cors);

    let listener = tokio::net::TcpListener::bind(&http_addr).await?;
    tracing::info!("Debug terminal available at http://{}/debug", http_addr);

    axum::serve(listener, app).await?;

    Ok(())
}

