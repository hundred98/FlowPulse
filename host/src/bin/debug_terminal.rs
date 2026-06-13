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
    response::{Html, IntoResponse, Json, sse::{Event, Sse}},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};
use axum::http::Method;

use emb_public::{CoreSocketClient, TemperatureManager, TemperatureManagerConfig, SyncEventPublisher, MeshManager, GpioManager};
use emb_api::ArcParamsApi;

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

/// Gcode print request
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GcodePrintRequest {
    filename: String,
}

/// Gcode print progress
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GcodePrintProgress {
    filename: String,
    current_line: usize,
    total_lines: usize,
    percent: f32,
    status: String,
    finished: bool,
    success: bool,
}

struct DebugState {
    core_client: Arc<CoreSocketClient>,
    /// GPIO Manager (handles pin control + event broadcast)
    gpio_manager: GpioManager,
    /// Temperature manager
    temperature_manager: Arc<TemperatureManager>,
    /// Mesh manager
    mesh_manager: MeshManager,
    /// Gcodes directory
    gcodes_dir: String,
    /// Print progress (shared between print task and progress polling)
    print_progress: Arc<tokio::sync::RwLock<Option<GcodePrintProgress>>>,
    /// Whether a print is currently running
    print_running: Arc<AtomicBool>,
}

fn create_debug_router(state: Arc<DebugState>) -> Router {
    Router::new()
        .route("/", get(debug_page))
        .route("/api/status", get(get_status))
        .route("/api/gpio/set", get(gpio_set))
        .route("/api/gpio/query", get(gpio_query))
        .route("/api/gpio/report/stream", get(gpio_report_stream))
        .route("/api/homing/start", get(homing_start))
        .route("/api/motion/gcode", post(gcode_execute))
        .route("/api/mesh/clear", post(mesh_clear))
        .route("/api/gcode/files", get(gcode_file_list))
        .route("/api/gcode/print", post(gcode_file_print))
        .route("/api/gcode/progress", get(gcode_print_progress))
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

async fn gpio_set(
    State(state): State<Arc<DebugState>>,
    axum::extract::Query(req): axum::extract::Query<GpioSetRequest>,
) -> impl IntoResponse {
    log::info!("Debug: GPIO set {} = {}", req.name, req.value);
    
    match state.gpio_manager.set_pin(&req.name, req.value).await {
        Ok(_) => Json(ApiResponse::success(GpioInfo { name: req.name, value: req.value })),
        Err(e) => Json(ApiResponse::<GpioInfo>::error(format!("Error: {}", e))),
    }
}

async fn gpio_query(
    State(state): State<Arc<DebugState>>,
    axum::extract::Query(req): axum::extract::Query<GpioQueryRequest>,
) -> impl IntoResponse {
    log::info!("Debug: GPIO query {}", req.name);
    
    match state.gpio_manager.query_pin(&req.name).await {
        Ok(value) => Json(ApiResponse::success(GpioInfo { name: req.name, value })),
        Err(e) => Json(ApiResponse::<GpioInfo>::error(format!("Error: {}", e))),
    }
}

/// GPIO Report SSE流
async fn gpio_report_stream(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    use std::convert::Infallible;
    use tokio_stream::StreamExt;
    
    let rx = state.gpio_manager.event_receiver();
    
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

/// Mesh Clear: 清除服务端的Mesh数据
async fn mesh_clear(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    log::info!("Mesh Clear: Clearing mesh data from server...");
    
    match state.mesh_manager.clear_mesh().await {
        Ok(()) => {
            log::info!("✅ Mesh cleared successfully");
            Json(ApiResponse::success("Mesh cleared successfully".to_string()))
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
    if cmd != "G0" && cmd != "G1" && cmd != "G2" && cmd != "G3" {
        return Json(ApiResponse::<String>::error(format!("Unsupported G-code: {}", cmd)));
    }
    
    // Parse parameters (X, Y, Z, E, F, I, J)
    let mut x: Option<f32> = None;
    let mut y: Option<f32> = None;
    let mut z: Option<f32> = None;
    let mut e: Option<f32> = None;
    let mut feed_rate: Option<f32> = None;
    let mut i: Option<f32> = None;
    let mut j: Option<f32> = None;
    
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
        } else if part.starts_with('I') {
            i = part[1..].parse().ok();
        } else if part.starts_with('J') {
            j = part[1..].parse().ok();
        }
    }
    
    // Build arc parameters for G2/G3
    let arc = if cmd == "G2" || cmd == "G3" {
        match (i, j) {
            (Some(i_val), Some(j_val)) => Some(ArcParamsApi {
                i: i_val,
                j: j_val,
                direction: if cmd == "G2" { 0 } else { 1 }, // 0=CW/G2, 1=CCW/G3
            }),
            (Some(i_val), None) => Some(ArcParamsApi {
                i: i_val,
                j: 0.0,
                direction: if cmd == "G2" { 0 } else { 1 },
            }),
            (None, Some(j_val)) => Some(ArcParamsApi {
                i: 0.0,
                j: j_val,
                direction: if cmd == "G2" { 0 } else { 1 },
            }),
            (None, None) => None, // G2/G3 without I/J will fail on server
        }
    } else {
        None
    };
    
    // Dispatch motion to server
    match state.core_client.motion_dispatch_arc(&cmd, x, y, z, e, feed_rate, arc).await {
        Ok(_) => {
            Json(ApiResponse::success("Motion dispatched".to_string()))
        }
        Err(e) => {
            log::error!("❌ Motion dispatch failed: {}", e);
            Json(ApiResponse::<String>::error(format!("Motion dispatch failed: {}", e)))
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
            log::warn!("Failed to read gcodes dir '{}': {}", dir, e);
            Json(ApiResponse::<Vec<String>>::error(format!("Cannot read directory: {}", e)))
        }
    }
}

/// Start printing a gcode file
async fn gcode_file_print(
    State(state): State<Arc<DebugState>>,
    Json(req): Json<GcodePrintRequest>,
) -> impl IntoResponse {
    // Check if already printing
    if state.print_running.load(Ordering::SeqCst) {
        return Json(ApiResponse::<String>::error("A print is already running".to_string()));
    }

    let file_path = format!("{}/{}", state.gcodes_dir, req.filename);
    log::info!("Starting print from file: {}", file_path);

    // Count total lines first
    let content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(e) => {
            return Json(ApiResponse::<String>::error(format!("Failed to read file: {}", e)));
        }
    };
    let total_lines = content.lines().count();

    if total_lines == 0 {
        return Json(ApiResponse::<String>::error("File is empty".to_string()));
    }

    // Set running flag
    state.print_running.store(true, Ordering::SeqCst);

    // Clone state for background task
    let state_clone = state.clone();
    let filename = req.filename.clone();

    // Spawn background print task
    tokio::spawn(async move {
        let result = run_gcode_print(&state_clone, &filename, &content, total_lines).await;

        let (status, success) = match &result {
            Ok(()) => ("Completed".to_string(), true),
            Err(e) => (format!("Error: {}", e), false),
        };

        // Update final progress
        let progress = GcodePrintProgress {
            filename: filename.clone(),
            current_line: total_lines,
            total_lines,
            percent: 100.0,
            status,
            finished: true,
            success,
        };
        *state_clone.print_progress.write().await = Some(progress);
        state_clone.print_running.store(false, Ordering::SeqCst);

        log::info!("Print {}: {}", filename, if result.is_ok() { "✅ completed" } else { "❌ failed" });
    });

    Json(ApiResponse::success(format!("Print started: {} ({} lines)", req.filename, total_lines)))
}

/// Poll print progress
async fn gcode_print_progress(
    State(state): State<Arc<DebugState>>,
) -> impl IntoResponse {
    let progress = state.print_progress.read().await;
    match progress.as_ref() {
        Some(p) => Json(ApiResponse::success(p.clone())),
        None => Json(ApiResponse::success(GcodePrintProgress {
            filename: String::new(),
            current_line: 0,
            total_lines: 0,
            percent: 0.0,
            status: "No print active".to_string(),
            finished: true,
            success: true,
        })),
    }
}

/// Run the actual gcode print: read file and dispatch commands
async fn run_gcode_print(
    state: &DebugState,
    filename: &str,
    content: &str,
    total_lines: usize,
) -> Result<(), String> {
    use emb_public::gcode::GCodeParser;
    use emb_public::gcode::CommandKind;

    let lines: Vec<&str> = content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let line = line.trim();

        // Skip empty and comment lines
        if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
            // Update progress
            let progress = GcodePrintProgress {
                filename: filename.to_string(),
                current_line: i + 1,
                total_lines,
                percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                status: format!("Skipping: {}", line),
                finished: false,
                success: true,
            };
            *state.print_progress.write().await = Some(progress);
            continue;
        }

        // Parse the line
        let cmd = match GCodeParser::parse_line(line, i as u32) {
            Some(parsed) => parsed,
            None => {
                log::warn!("Parse error line {}: {}", i + 1, line);
                let progress = GcodePrintProgress {
                    filename: filename.to_string(),
                    current_line: i + 1,
                    total_lines,
                    percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                    status: format!("Parse error: {}", line),
                    finished: false,
                    success: false,
                };
                *state.print_progress.write().await = Some(progress);
                continue;
            }
        };

        match &cmd.kind {
            CommandKind::Motion(motion_cmd) => {
                let (cmd_str, x, y, z, e, f, arc) = match motion_cmd {
                    emb_public::gcode::MotionCommand::LinearMove { x, y, z, e, f, is_rapid } => {
                        let cmd_str = if *is_rapid { "G0" } else { "G1" };
                        (cmd_str.to_string(), *x, *y, *z, *e, *f, None)
                    }
                    emb_public::gcode::MotionCommand::ArcMove { x, y, z, e, f, i, j, is_cw } => {
                        let cmd_str = if *is_cw { "G2" } else { "G3" };
                        let arc = Some(emb_api::ArcParamsApi {
                            i: *i,
                            j: *j,
                            direction: if *is_cw { 0 } else { 1 },
                        });
                        (cmd_str.to_string(), *x, *y, *z, *e, *f, arc)
                    }
                    _ => {
                        log::info!("  → Skipping non-motion command: {:?}", motion_cmd);
                        continue;
                    }
                };

                match state.core_client.motion_dispatch_arc(&cmd_str, x, y, z, e, f, arc).await {
                    Ok(()) => {
                        let progress = GcodePrintProgress {
                            filename: filename.to_string(),
                            current_line: i + 1,
                            total_lines,
                            percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                            status: format!("Dispatched: {}", line),
                            finished: false,
                            success: true,
                        };
                        *state.print_progress.write().await = Some(progress);
                    }
                    Err(e) => {
                        log::warn!("Motion dispatch failed line {}: {}", i + 1, e);
                        let progress = GcodePrintProgress {
                            filename: filename.to_string(),
                            current_line: i + 1,
                            total_lines,
                            percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                            status: format!("Motion error: {}", e),
                            finished: false,
                            success: false,
                        };
                        *state.print_progress.write().await = Some(progress);
                        return Err(format!("Motion dispatch failed at line {}: {}", i + 1, e));
                    }
                }
            }
            CommandKind::Machine(m_cmd) => {
                use emb_api::MExecutionType;

                let exec_type = m_cmd.execution_type();

                // For SyncWait (M109/M190): send command, server handles temperature monitoring.
                // Server uses "fake blocking" — it filters allowed commands and responds
                // when temperature is reached (or timeout).
                if exec_type == MExecutionType::SyncWait {
                    // Determine heater and target temperature
                    let (heater, target) = match &m_cmd {
                        emb_api::MCommand::WaitHotendTemp { temp, .. } => ("hotend", *temp),
                        emb_api::MCommand::WaitBedTemp { temp } => ("bed", *temp),
                        _ => unreachable!(),
                    };

                    // Update local cache so safety check knows heating is intentional
                    state.temperature_manager.update_target_cache(heater, target).await;

                    // Use server's timeout + 3s buffer for client-side timeout
                    // Server timeout is 300s (from config/temperature.json)
                    let server_timeout_secs = 300u64;
                    let timeout = std::time::Duration::from_secs(server_timeout_secs + 3);

                    match state.core_client.motion_execute_m_command_with_timeout(m_cmd.clone(), timeout).await {
                        Ok(()) => {
                            // Server returned success immediately (command sent to device).
                            // Now wait locally for temperature to reach target.
                            log::info!("  → Command sent, waiting for {} to reach {:.1}°C...", heater, target);

                            let wait_start = std::time::Instant::now();
                            let wait_timeout = std::time::Duration::from_secs(server_timeout_secs);
                            let wait_tolerance = 2.0;
                            let wait_check = std::time::Duration::from_millis(500);
                            let wait_stable_count = 3u32;
                            let mut current_stable = 0u32;

                            loop {
                                if wait_start.elapsed() > wait_timeout {
                                    log::warn!("Temperature wait timeout for {} ({}s)", heater, server_timeout_secs);
                                    let progress = GcodePrintProgress {
                                        filename: filename.to_string(),
                                        current_line: i + 1,
                                        total_lines,
                                        percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                                        status: format!("Temp timeout: {} = {:.1}°C", heater, target),
                                        finished: false,
                                        success: false,
                                    };
                                    *state.print_progress.write().await = Some(progress);
                                    return Err(format!("Temperature wait timeout for {} after {}s", heater, server_timeout_secs));
                                }

                                let status = state.temperature_manager.get_temp_status().await;
                                let current = if heater == "bed" { status.bed_current } else { status.hotend_current };

                                if current >= target - wait_tolerance {
                                    current_stable += 1;
                                    let elapsed = wait_start.elapsed().as_secs();
                                    log::info!("Temperature stable {}/{}: {} = {:.1}°C (target {:.1}°C, elapsed {}s)",
                                        current_stable, wait_stable_count, heater, current, target, elapsed);

                                    if current_stable >= wait_stable_count {
                                        log::info!("  ✅ Temperature reached: {} = {:.1}°C (elapsed {}s)", heater, current, elapsed);
                                        break;
                                    }
                                } else {
                                    if current_stable > 0 {
                                        current_stable = 0;
                                    }
                                }

                                tokio::time::sleep(wait_check).await;
                            }

                            let progress = GcodePrintProgress {
                                filename: filename.to_string(),
                                current_line: i + 1,
                                total_lines,
                                percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                                status: format!("Temp OK: {} = {}°C", heater, target),
                                finished: false,
                                success: true,
                            };
                            *state.print_progress.write().await = Some(progress);
                        }
                        Err(e) => {
                            log::warn!("M command failed line {}: {} — {}", i + 1, line, e);
                            let progress = GcodePrintProgress {
                                filename: filename.to_string(),
                                current_line: i + 1,
                                total_lines,
                                percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                                status: format!("M cmd error: {}", e),
                                finished: false,
                                success: false,
                            };
                            *state.print_progress.write().await = Some(progress);
                            return Err(format!("M command failed at line {}: {}", i + 1, e));
                        }
                    }
                } else if exec_type == MExecutionType::Query {
                    // M105 / QueryTemperature: read from cache, don't go to server
                    let heaters = state.temperature_manager.get_all_heaters().await;
                    log::info!("  → Temperature query (cached):");
                    for (name, state) in &heaters {
                        log::info!("      {}: {:.1}°C / {:.1}°C", name, state.current_temp, state.target_temp);
                    }
                    let progress = GcodePrintProgress {
                        filename: filename.to_string(),
                        current_line: i + 1,
                        total_lines,
                        percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                        status: format!("Temp query (cached): {} heaters", heaters.len()),
                        finished: false,
                        success: true,
                    };
                    *state.print_progress.write().await = Some(progress);
                } else {
                    // Other M commands (SyncSet, MotionParam): send to server and continue
                    // For temperature SyncSet commands (M104, M140), also update local target cache
                    let is_temp_cmd = matches!(m_cmd,
                        emb_api::MCommand::SetHotendTemp { .. } |
                        emb_api::MCommand::SetBedTemp { .. }
                    );

                    match state.core_client.motion_execute_m_command(m_cmd.clone()).await {
                        Ok(()) => {
                            // After successful server-side execution, update local temperature cache
                            if is_temp_cmd {
                                match m_cmd {
                                    emb_api::MCommand::SetHotendTemp { temp, .. } => {
                                        state.temperature_manager.update_target_cache("hotend", *temp).await;
                                    }
                                    emb_api::MCommand::SetBedTemp { temp } => {
                                        state.temperature_manager.update_target_cache("bed", *temp).await;
                                    }
                                    _ => {}
                                }
                            }
                            let progress = GcodePrintProgress {
                                filename: filename.to_string(),
                                current_line: i + 1,
                                total_lines,
                                percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                                status: format!("Executed: {}", line),
                                finished: false,
                                success: true,
                            };
                            *state.print_progress.write().await = Some(progress);
                        }
                        Err(e) => {
                            log::warn!("M command failed line {}: {} — {}", i + 1, line, e);
                            let progress = GcodePrintProgress {
                                filename: filename.to_string(),
                                current_line: i + 1,
                                total_lines,
                                percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                                status: format!("M cmd error: {}", e),
                                finished: false,
                                success: false,
                            };
                            *state.print_progress.write().await = Some(progress);
                            return Err(format!("M command failed at line {}: {}", i + 1, e));
                        }
                    }
                }
            }
            _ => {
                // Other command types (G28, G29, etc.) - skip for now
                log::info!("  → Skipping (unsupported): {:?}", cmd);
                let progress = GcodePrintProgress {
                    filename: filename.to_string(),
                    current_line: i + 1,
                    total_lines,
                    percent: ((i + 1) as f32 / total_lines as f32) * 100.0,
                    status: format!("Skipped: {:?}", cmd),
                    finished: false,
                    success: true,
                };
                *state.print_progress.write().await = Some(progress);
            }
        }

        // Small delay between commands
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        "info,emb_public::config::config_protocol=debug,debug_terminal=debug"
    )).init();

    let args: Vec<String> = std::env::args().collect();
    let http_addr = args.get(1).unwrap_or(&"127.0.0.1:8080".to_string()).clone();
    let core_addr = args.get(2).unwrap_or(&"127.0.0.1:9527".to_string()).clone();

    log::info!("Debug Terminal starting...");
    log::info!("HTTP server: {}", http_addr);
    log::info!("Core server: {}", core_addr);

    // Step 1: Load all configuration files at once
    setup::load_all_configs(setup::CONFIG_DIR)?;

    // Step 2: Create host and connect to emb-core-server
    let host = setup::create_and_connect_host(&core_addr).await?;

    // 创建GPIO Manager（内部包含 broadcast channel）
    let gpio_manager = GpioManager::new(host.client());

    // 设置GPIO Report回调（通过 GpioManager 转发到 broadcast channel）
    gpio_manager.setup_callback().await;

    // Step 3: Initialize device (serial, GPIO subscribe, configs, STM32 seq)
    // 注意：setup::initialize_device 内部会调用 gpio_subscribe_report
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

    // 后台 pinger：每2秒 ping 一次，触发 read_response 消费 GPIO Report 推送
    let ping_client = host.client();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = ping_client.ping().await;
        }
    });

    // 后台安全检查
    let tm_safety = temperature_manager.clone();
    tokio::spawn(async move {
        tm_safety.start_safety_check_loop().await;
    });

    // Gcodes directory (default: ./gcodes relative to current working dir)
    let gcodes_dir = std::env::current_dir()
        .map(|p| p.join("gcodes").to_string_lossy().to_string())
        .unwrap_or_else(|_| "gcodes".to_string());

    // 打印状态
    let print_progress: Arc<tokio::sync::RwLock<Option<GcodePrintProgress>>> = Arc::new(tokio::sync::RwLock::new(None));
    let print_running = Arc::new(AtomicBool::new(false));

    let state = Arc::new(DebugState { 
        core_client: host.client(),
        gpio_manager,
        temperature_manager,
        mesh_manager: MeshManager::new(host.client()),
        gcodes_dir,
        print_progress,
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
    log::info!("Debug terminal available at http://{}/debug", http_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
