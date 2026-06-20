use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::gcode::GCodeFileParser;
use crate::common::EmbResult;
use crate::print_control::checkpoint::{CheckpointManager, CheckpointContext, build_resume_commands};
use crate::state::DeviceStateManager;
use crate::safety::SafetyController;
use crate::temperature::TemperaturePreset;
use crate::core_client::CoreSocketClient;
use crate::temperature::TemperatureManager;
use crate::gcode::{GCodeParser, CommandKind, MotionCommand};
use super::state_machine::PrintStateMachine;
use emb_api::{MExecutionType, MCommand, ArcParamsApi};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrintState {
    Idle,
    Starting,
    Printing,
    Paused,
    Resuming,
    Stopping,
    Completed,
    Failed,
}

impl PrintState {
    pub fn is_active(&self) -> bool {
        matches!(self, PrintState::Printing | PrintState::Starting | PrintState::Resuming)
    }
    
    pub fn can_pause(&self) -> bool {
        matches!(self, PrintState::Printing)
    }
    
    pub fn can_resume(&self) -> bool {
        matches!(self, PrintState::Paused)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintJob {
    pub id: String,
    pub file_name: String,
    pub file_path: String,
    pub name: Option<String>,
    pub material: String,
    pub estimated_time_seconds: u64,
    #[serde(skip)]
    pub created_at: u64,
    #[serde(skip)]
    pub started_at: Option<u64>,
    #[serde(skip)]
    pub completed_at: Option<u64>,
}

impl PrintJob {
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            file_name: String::new(),
            file_path: String::new(),
            name: None,
            material: String::new(),
            estimated_time_seconds: 0,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            started_at: None,
            completed_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MotionConfig {
    pub max_feedrate_x: f32,
    pub max_feedrate_y: f32,
    pub max_feedrate_z: f32,
    pub max_feedrate_e: f32,
    pub acceleration: f32,
    pub retract_acceleration: f32,
    pub travel_acceleration: f32,
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            max_feedrate_x: 500.0,
            max_feedrate_y: 500.0,
            max_feedrate_z: 12.0,
            max_feedrate_e: 25.0,
            acceleration: 980.0,
            retract_acceleration: 980.0,
            travel_acceleration: 980.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SafetyConfig {
    pub min_temp_hotend: f32,
    pub max_temp_hotend: f32,
    pub min_temp_bed: f32,
    pub max_temp_bed: f32,
    pub min_extrude_temp: f32,
    pub watch_period_ms: u32,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            min_temp_hotend: 0.0,
            max_temp_hotend: 300.0,
            min_temp_bed: 0.0,
            max_temp_bed: 120.0,
            min_extrude_temp: 170.0,
            watch_period_ms: 20000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PrintEvent {
    Started,
    Paused,
    Resumed,
    Completed,
    Failed(String),
    Progress { percent: f32, layer: u32 },
}

/// Print progress tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintProgress {
    /// Progress percentage (0-100)
    pub percent: f32,
    
    /// Current layer number
    pub current_layer: u32,
    
    /// Total layer count
    pub total_layers: u32,
    
    /// Elapsed time in seconds
    pub elapsed_seconds: u64,
    
    /// Estimated remaining time in seconds
    pub remaining_seconds: u64,

    /// Current G-code line index (for print execution loop)
    pub current_line: u32,
    /// Total number of G-code lines (for print execution loop)
    pub total_lines: u32,
    /// Status message for current execution step
    pub status: String,
}

impl Default for PrintProgress {
    fn default() -> Self {
        Self {
            percent: 0.0,
            current_layer: 0,
            total_layers: 0,
            elapsed_seconds: 0,
            remaining_seconds: 0,
            current_line: 0,
            total_lines: 0,
            status: String::new(),
        }
    }
}

/// Serializable checkpoint snapshot returned by `PrintController::checkpoint_info()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSnapshot {
    pub exists: bool,
    pub file_path: String,
    pub line: u32,
    pub z_pos: f32,
    pub hotend_temp: f32,
    pub bed_temp: f32,
    pub feed_rate: u16,
    pub flow_rate: u16,
    pub fan_speed: u8,
}

impl Default for CheckpointSnapshot {
    fn default() -> Self {
        Self {
            exists: false,
            file_path: String::new(),
            line: 0,
            z_pos: 0.0,
            hotend_temp: 0.0,
            bed_temp: 0.0,
            feed_rate: 100,
            flow_rate: 100,
            fan_speed: 0,
        }
    }
}

pub struct PrintController {
    state_machine: Arc<RwLock<PrintStateMachine>>,
    current_job: Arc<RwLock<Option<PrintJob>>>,
    gcode_parser: Arc<RwLock<Option<GCodeFileParser>>>,
    presets: Arc<RwLock<Vec<TemperaturePreset>>>,
    #[allow(dead_code)]
    motion_config: Arc<RwLock<MotionConfig>>,
    #[allow(dead_code)]
    safety_config: Arc<RwLock<SafetyConfig>>,
    stop_requested: Arc<AtomicBool>,
    
    // New: Device state manager
    device_state: Option<Arc<DeviceStateManager>>,
    
    // New: Safety controller
    safety_controller: Option<Arc<SafetyController>>,
    
    // New: Print progress
    progress: Arc<RwLock<PrintProgress>>,

    // New: Core socket client for motion/temperature commands
    client: Option<Arc<CoreSocketClient>>,
    // New: Temperature manager for temperature control during print
    temperature_manager: Option<Arc<TemperatureManager>>,
    // Power-loss resume checkpoint manager (optional)
    checkpoint_manager: Option<Arc<Mutex<CheckpointManager>>>,
}

impl PrintController {
    pub fn new() -> Self {
        Self {
            state_machine: Arc::new(RwLock::new(PrintStateMachine::new())),
            current_job: Arc::new(RwLock::new(None)),
            gcode_parser: Arc::new(RwLock::new(None)),
            presets: Arc::new(RwLock::new(vec![
                TemperaturePreset::default(),
                TemperaturePreset {
                    name: "ABS".to_string(),
                    hotend_temp: 240.0,
                    bed_temp: 100.0,
                    chamber_temp: Some(50.0),
                    fan_speed: 0,
                },
            ])),
            motion_config: Arc::new(RwLock::new(MotionConfig::default())),
            safety_config: Arc::new(RwLock::new(SafetyConfig::default())),
            stop_requested: Arc::new(AtomicBool::new(false)),
            device_state: None,
            safety_controller: None,
            progress: Arc::new(RwLock::new(PrintProgress::default())),
            client: None,
            temperature_manager: None,
            checkpoint_manager: None,
        }
    }
    
    /// Create PrintController with DeviceStateManager and SafetyController
    pub fn with_state_management(
        device_state: Arc<DeviceStateManager>,
        safety_controller: Arc<SafetyController>,
    ) -> Self {
        let mut controller = Self::new();
        controller.device_state = Some(device_state);
        controller.safety_controller = Some(safety_controller);
        controller
    }
    
    /// Set device state manager
    pub fn set_device_state(&mut self, device_state: Arc<DeviceStateManager>) {
        self.device_state = Some(device_state);
    }
    
    /// Set safety controller
    pub fn set_safety_controller(&mut self, safety_controller: Arc<SafetyController>) {
        self.safety_controller = Some(safety_controller);
    }

    /// Set core socket client (required for print execution)
    pub fn set_client(&mut self, client: Arc<CoreSocketClient>) {
        self.client = Some(client);
    }

    /// Set temperature manager (required for print execution)
    pub fn set_temperature_manager(&mut self, temperature_manager: Arc<TemperatureManager>) {
        self.temperature_manager = Some(temperature_manager);
    }

    /// Set power-loss resume checkpoint manager.
    /// Creates or loads `resume.json` from the given path.
    pub fn set_resume_path(&mut self, path: &str) {
        self.checkpoint_manager = Some(Arc::new(Mutex::new(CheckpointManager::new(path))));
    }

    /// Check if a checkpoint exists for the given file path.
    pub fn has_checkpoint_for(&self, file_path: &str) -> bool {
        self.checkpoint_manager.as_ref().map_or(false, |cm| {
            let mgr = cm.lock().unwrap();
            mgr.has_checkpoint() && mgr.checkpoint_data().file_path == file_path
        })
    }

    /// Get resume G-code commands for the current checkpoint, if any.
    pub fn get_resume_commands(&self) -> Vec<String> {
        self.checkpoint_manager.as_ref().map_or_else(Vec::new, |cm| {
            let mgr = cm.lock().unwrap();
            if mgr.has_checkpoint() {
                build_resume_commands(&mgr.checkpoint_data(), &mgr.config())
            } else {
                Vec::new()
            }
        })
    }

    /// Clear the current checkpoint.
    pub fn clear_checkpoint(&self) -> EmbResult<()> {
        if let Some(ref cm) = self.checkpoint_manager {
            let mut mgr = cm.lock().unwrap();
            mgr.clear()?;
        }
        Ok(())
    }

    /// Return a snapshot of the current checkpoint (if any).
    pub fn checkpoint_info(&self) -> Option<CheckpointSnapshot> {
        self.checkpoint_manager.as_ref().and_then(|cm| {
            let mgr = cm.lock().unwrap();
            if mgr.has_checkpoint() {
                let cp = mgr.checkpoint_data();
                Some(CheckpointSnapshot {
                    exists: true,
                    file_path: cp.file_path,
                    line: cp.line,
                    z_pos: cp.z_pos,
                    hotend_temp: cp.hotend_temp,
                    bed_temp: cp.bed_temp,
                    feed_rate: cp.feed_rate,
                    flow_rate: cp.flow_rate,
                    fan_speed: cp.fan_speed,
                })
            } else {
                Some(CheckpointSnapshot {
                    exists: false,
                    ..Default::default()
                })
            }
        })
    }

    /// Execute G-code file print loop.
    ///
    /// Processes each line of the G-code file, dispatching motion commands,
    /// temperature commands (M104/M140/M109/M190), and handling queries (M105).
    /// Updates progress after each command.
    ///
    /// # Arguments
    /// * `filename` - Display name for logging
    /// * `content` - Full G-code file content
    /// * `total_lines` - Total number of lines
    ///
    /// # Returns
    /// * `Ok(())` - If the print completed successfully
    /// * `Err(String)` - If any command failed
    pub async fn execute_print_loop(&self, filename: &str, file_path: &str, resume: bool) -> Result<(), String> {
        use std::sync::atomic::{AtomicU16, Ordering};
        use std::io::{BufRead, Read};

        let client = self.client.as_ref().ok_or("PrintController: CoreSocketClient not set")?.clone();
        let temperature_manager = self.temperature_manager.as_ref().ok_or("PrintController: TemperatureManager not set")?;

        // Pass 1: count total lines and valid (non-comment, non-empty) lines.
        // Uses BufReader to avoid loading the entire file into memory.
        let file = std::fs::File::open(file_path).map_err(|e| format!("Failed to open file '{}': {}", file_path, e))?;
        let reader = std::io::BufReader::new(file);
        let mut total_lines: u32 = 0;
        let mut total_valid: u32 = 0;
        for line_result in reader.lines() {
            let line = line_result.map_err(|e| format!("Failed to read '{}': {}", file_path, e))?;
            total_lines += 1;
            let t = line.trim();
            if !t.is_empty() && !t.starts_with(';') && !t.starts_with("//") {
                total_valid += 1;
            }
        }

        if total_valid == 0 {
            return Err("No valid G-code commands found in file".to_string());
        }

        tracing::info!(
            "Starting print execution: {} ({} total lines, {} valid commands)",
            filename, total_lines, total_valid
        );

        // Register buf_time callback to track MCU execution progress.
        // Initialize to u16::MAX (sentinel) meaning "no 0x1A data yet".
        // Until the first real 0x1A arrives, we assume MCU buffer is full,
        // which forces aggressive correction and prevents instant 100% on small files.
        let mcu_buf_time_ms = Arc::new(AtomicU16::new(u16::MAX));
        {
            let bt = mcu_buf_time_ms.clone();
            client.set_buf_time_callback(move |buf_time_ms, _free_slots| {
                bt.store(buf_time_ms, Ordering::Relaxed);
            }).await;
        }

        // Send EnterPrintMode to MCU to enable 0x1A status reporting.
        // Without this, the MCU will not send buf_time updates, and the
        // client-side drain detection via buf_time will not work.
        tracing::info!("Sending EnterPrintMode to MCU...");
        client.serial_enter_print_mode().await.map_err(|e| {
            tracing::error!("EnterPrintMode failed: {}", e);
            format!("EnterPrintMode failed: {}", e)
        })?;

        // Pass 2: process lines sequentially using BufReader (lazy I/O, ~100 lines per buffer).
        let file = std::fs::File::open(file_path).map_err(|e| format!("Failed to open '{}': {}", file_path, e))?;
        let mut reader = std::io::BufReader::new(file);
        let mut valid_processed: u32 = 0;
        let mut current_line: u32 = 0;

        // Position and mode tracking for power-loss resume checkpoints.
        let mut position_x = 0.0_f32;
        let mut position_y = 0.0_f32;
        let mut position_z = 0.0_f32;
        let mut position_e = 0.0_f32;
        let mut is_absolute = true;
        let mut e_is_relative = true;
        let mut feed_rate = 100_u16;
        let mut flow_rate = 100_u16;
        let mut fan_speed = 0_u8;
        // Flag: when set, force an immediate checkpoint save regardless of interval.
        let mut save_immediately = false;

        // ── Resume support ──
        // If requested and a valid checkpoint exists for this file, send recovery
        // G-code commands (heat, home, position, etc.) before resuming, then skip
        // the BufReader past the already-processed lines.
        if resume {
            let has_cp = self.checkpoint_manager.as_ref().map_or(false, |cm| {
                let mgr = cm.lock().unwrap();
                mgr.has_checkpoint() && mgr.checkpoint_data().file_path == file_path
            });
            if !has_cp {
                return Err(format!(
                    "Resume requested but no checkpoint found for file '{}'",
                    file_path
                ));
            }
            // Send resume G-code commands
            if let Some(ref cm) = self.checkpoint_manager {
                let (commands, cp_line) = {
                    let mgr = cm.lock().unwrap();
                    let cp = mgr.checkpoint_data();
                    let cmd_list = build_resume_commands(&cp, &mgr.config());
                    let skip_line = cp.line;
                    (cmd_list, skip_line)
                };
                tracing::info!(
                    "Resuming print from line {} ({} commands to skip)",
                    cp_line, cp_line
                );
                // Send each resume command to the MCU
                for cmd in &commands {
                    let line = format!("{}\n", cmd);
                    client.serial_send_raw(line.as_bytes()).await
                        .map_err(|e| format!("Resume command failed: {} — {}", cmd, e))?;
                }
                // Skip BufReader past already-processed lines
                // During the skip, capture the last E value seen
                let mut last_e: Option<f32> = None;
                let mut skipped: u32 = 0;
                for line_result in reader.by_ref().lines() {
                    let l = line_result.map_err(|e| format!("Skip read error: {}", e))?;
                    // Extract E value from the line
                    if let Some(e_val) = extract_e_value(&l) {
                        last_e = Some(e_val);
                    }
                    skipped += 1;
                    if skipped >= cp_line {
                        break;
                    }
                }
                // If no E value found in the skipped range, scan forward
                // up to 200 lines using a separate file handle.
                if last_e.is_none() {
                    if let Ok(scan_file) = std::fs::File::open(file_path) {
                        let scan_reader = std::io::BufReader::new(scan_file);
                        let mut scan_skipped: u32 = 0;
                        let mut scanned: u32 = 0;
                        for line_result in scan_reader.lines() {
                            if let Ok(l) = line_result {
                                scan_skipped += 1;
                                if scan_skipped <= cp_line {
                                    continue; // skip already-processed lines
                                }
                                if scanned >= 200 {
                                    break;
                                }
                                scanned += 1;
                                if let Some(e_val) = extract_e_value(&l) {
                                    last_e = Some(e_val);
                                    tracing::info!(
                                        "Found E value {:.4} at line {} ({} lines ahead of checkpoint)",
                                        e_val, cp_line + scanned, scanned
                                    );
                                    break;
                                }
                            }
                        }
                    } else {
                        tracing::warn!("Could not re-open '{}' for E-value forward scan", file_path);
                    }
                }
                // Send G92 E to restore the extruder position
                if let Some(e_val) = last_e {
                    let e_cmd = format!("G92 E{:.4}\n", e_val);
                    client.serial_send_raw(e_cmd.as_bytes()).await
                        .map_err(|e| format!("Resume G92 E failed: {}", e))?;
                    tracing::info!("Restored E position to {:.4}", e_val);
                } else {
                    tracing::warn!(
                        "No E value found in skipped lines or up to 200 lines ahead; extruder position not restored"
                    );
                }
                current_line = cp_line;
                tracing::info!("Skipped to line {}, resuming command dispatch", current_line);
            }
        }

        for line_result in reader.lines() {
            let line_raw = line_result.map_err(|e| format!("Read error at line {}: {}", current_line + 1, e))?;
            current_line += 1;

            // Check for stop request
            if self.stop_requested.load(Ordering::SeqCst) {
                tracing::info!("Print stopped by user request at line {}", current_line);
                return Err("Print stopped by user".to_string());
            }

            let line = line_raw.trim();

            // Skip empty and comment lines — do NOT advance valid_processed or percent.
            if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
                let mut progress = self.progress.write().await;
                progress.current_line = current_line;
                progress.total_lines = total_lines;
                progress.status = format!("Skipping: {}", line);
                self.sync_progress_to_device_state().await;
                continue;
            }

            valid_processed += 1;

            // Parse the line
            let cmd = match GCodeParser::parse_line(line, current_line) {
                Some(parsed) => parsed,
                None => {
                    tracing::warn!("Parse error line {}: {}", current_line, line);
                    let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                    let mut progress = self.progress.write().await;
                    progress.percent = percent;
                    progress.current_line = current_line;
                    progress.total_lines = total_lines;
                    progress.status = format!("Parse error: {}", line);
                    self.sync_progress_to_device_state().await;
                    continue;
                }
            };

            match &cmd.kind {
                CommandKind::Motion(motion_cmd) => {
                    match motion_cmd {
                        MotionCommand::LinearMove { x, y, z, e, f, is_rapid } => {
                            // Track position for checkpoint
                            position_x = x.unwrap_or(position_x);
                            position_y = y.unwrap_or(position_y);
                            position_z = z.unwrap_or(position_z);
                            position_e = e.unwrap_or(position_e);
                            let cmd_str = if *is_rapid { "G0" } else { "G1" };
                            match client.motion_dispatch_arc(cmd_str, *x, *y, *z, *e, *f, None).await {
                                Ok(_) => {
                                    let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                                    let mut progress = self.progress.write().await;
                                    progress.percent = percent;
                                    progress.current_line = current_line;
                                    progress.total_lines = total_lines;
                                    progress.status = format!("Dispatched: {}", line);
                                }
                                Err(e) => {
                                    tracing::warn!("Motion dispatch failed line {}: {}", current_line, e);
                                    let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                                    let mut progress = self.progress.write().await;
                                    progress.percent = percent;
                                    progress.current_line = current_line;
                                    progress.total_lines = total_lines;
                                    progress.status = format!("Motion error: {}", e);
                                    return Err(format!("Motion dispatch failed at line {}: {}", current_line, e));
                                }
                            }
                        }
                        MotionCommand::ArcMove { x, y, z, e, f, i, j, is_cw } => {
                            // Track position for checkpoint
                            position_x = x.unwrap_or(position_x);
                            position_y = y.unwrap_or(position_y);
                            position_z = z.unwrap_or(position_z);
                            position_e = e.unwrap_or(position_e);
                            let cmd_str = if *is_cw { "G2" } else { "G3" };
                            let arc = Some(ArcParamsApi {
                                i: *i,
                                j: *j,
                                direction: if *is_cw { 0 } else { 1 },
                            });
                            match client.motion_dispatch_arc(cmd_str, *x, *y, *z, *e, *f, arc).await {
                                Ok(_) => {
                                    let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                                    let mut progress = self.progress.write().await;
                                    progress.percent = percent;
                                    progress.current_line = current_line;
                                    progress.total_lines = total_lines;
                                    progress.status = format!("Dispatched: {}", line);
                                }
                                Err(e) => {
                                    tracing::warn!("Arc dispatch failed line {}: {}", current_line, e);
                                    let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                                    let mut progress = self.progress.write().await;
                                    progress.percent = percent;
                                    progress.current_line = current_line;
                                    progress.total_lines = total_lines;
                                    progress.status = format!("Arc error: {}", e);
                                    return Err(format!("Arc dispatch failed at line {}: {}", current_line, e));
                                }
                            }
                        }
                        MotionCommand::Dwell { dwell_time_ms } => {
                            match client.motion_dwell(*dwell_time_ms).await {
                                Ok(()) => {
                                    let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                                    let mut progress = self.progress.write().await;
                                    progress.percent = percent;
                                    progress.current_line = current_line;
                                    progress.total_lines = total_lines;
                                    progress.status = format!("Dwell: {}ms", dwell_time_ms);
                                }
                                Err(e) => {
                                    tracing::warn!("Dwell failed line {}: {}", current_line, e);
                                    return Err(format!("Dwell failed at line {}: {}", current_line, e));
                                }
                            }
                        }
                        _ => {
                            // Update position/mode tracking for checkpoint
                            match motion_cmd {
                                MotionCommand::SetPosition { x, y, z, e } => {
                                    position_x = x.unwrap_or(position_x);
                                    position_y = y.unwrap_or(position_y);
                                    position_z = z.unwrap_or(position_z);
                                    position_e = e.unwrap_or(position_e);
                                    save_immediately = true;
                                }
                                MotionCommand::AbsolutePositioning => is_absolute = true,
                                MotionCommand::RelativePositioning => is_absolute = false,
                                _ => {}
                            }
                            tracing::info!("  → Skipping non-motion command: {:?}", motion_cmd);
                            let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                            let mut progress = self.progress.write().await;
                            progress.percent = percent;
                            progress.current_line = current_line;
                            progress.total_lines = total_lines;
                            progress.status = format!("Skipping non-motion");
                        }
                    }
                }
                CommandKind::Machine(m_cmd) => {
                    let exec_type = m_cmd.execution_type();

                    // SyncWait (M109/M190): send command, server handles temperature monitoring
                    if exec_type == MExecutionType::SyncWait {
                        let (heater, target) = match &m_cmd {
                            MCommand::WaitHotendTemp { temp, .. } => ("hotend", *temp),
                            MCommand::WaitBedTemp { temp } => ("bed", *temp),
                            _ => unreachable!(),
                        };

                        // Update local cache so safety check knows heating is intentional
                        temperature_manager.update_target_cache(heater, target).await;

                        let server_timeout_secs = 300u64;
                        let timeout = std::time::Duration::from_secs(server_timeout_secs + 3);

                        match client.motion_execute_m_command_with_timeout(m_cmd.clone(), timeout).await {
                            Ok(()) => {
                                tracing::info!("  → Command sent, waiting for {} to reach {:.1}°C...", heater, target);

                                let wait_start = std::time::Instant::now();
                                let wait_timeout = std::time::Duration::from_secs(server_timeout_secs);
                                let wait_tolerance = 2.0;
                                let wait_check = std::time::Duration::from_millis(500);
                                let wait_stable_count = 3u32;
                                let mut current_stable = 0u32;

                                loop {
                                    if self.stop_requested.load(Ordering::SeqCst) {
                                        return Err("Print stopped during temperature wait".to_string());
                                    }

                                    if wait_start.elapsed() > wait_timeout {
                                        tracing::warn!("Temperature wait timeout for {} ({}s)", heater, server_timeout_secs);
                                        let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                                        let mut progress = self.progress.write().await;
                                        progress.percent = percent;
                                        progress.current_line = current_line;
                                        progress.total_lines = total_lines;
                                        progress.status = format!("Temp timeout: {} = {:.1}°C", heater, target);
                                        return Err(format!("Temperature wait timeout for {} after {}s", heater, server_timeout_secs));
                                    }

                                    let status = temperature_manager.get_temp_status().await;
                                    let current = if heater == "bed" { status.bed_current } else { status.hotend_current };

                                    if current >= target - wait_tolerance {
                                        current_stable += 1;
                                        let elapsed = wait_start.elapsed().as_secs();
                                        tracing::info!("Temperature stable {}/{}: {} = {:.1}°C (target {:.1}°C, elapsed {}s)",
                                            current_stable, wait_stable_count, heater, current, target, elapsed);

                                        if current_stable >= wait_stable_count {
                                            tracing::info!("  ✅ Temperature reached: {} = {:.1}°C (elapsed {}s)", heater, current, elapsed);
                                            break;
                                        }
                                    } else if current_stable > 0 {
                                        current_stable = 0;
                                    }

                                    tokio::time::sleep(wait_check).await;
                                }

                                let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                                let mut progress = self.progress.write().await;
                                progress.percent = percent;
                                progress.current_line = current_line;
                                progress.total_lines = total_lines;
                                progress.status = format!("Temp OK: {} = {}°C", heater, target);
                            }
                            Err(e) => {
                                tracing::warn!("M command failed line {}: {} — {}", current_line, line, e);
                                let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                                let mut progress = self.progress.write().await;
                                progress.percent = percent;
                                progress.current_line = current_line;
                                progress.total_lines = total_lines;
                                progress.status = format!("M cmd error: {}", e);
                                return Err(format!("M command failed at line {}: {}", current_line, e));
                            }
                        }
                    } else if exec_type == MExecutionType::Query {
                        // M105: read from cache only
                        let heaters = temperature_manager.get_all_heaters().await;
                        tracing::info!("  → Temperature query (cached):");
                        for (name, s) in &heaters {
                            tracing::info!("      {}: {:.1}°C / {:.1}°C", name, s.current_temp, s.target_temp);
                        }
                        let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                        let mut progress = self.progress.write().await;
                        progress.percent = percent;
                        progress.current_line = current_line;
                        progress.total_lines = total_lines;
                        progress.status = format!("Temp query (cached): {} heaters", heaters.len());
                    } else {
                        // Other M commands (SyncSet, MotionParam): send to server
                        let is_temp_cmd = matches!(m_cmd,
                            MCommand::SetHotendTemp { .. } |
                            MCommand::SetBedTemp { .. }
                        );

                        match client.motion_execute_m_command(m_cmd.clone()).await {
                            Ok(()) => {
                                // Update local temperature cache after successful execution
                                if is_temp_cmd {
                                    match m_cmd {
                                        MCommand::SetHotendTemp { temp, .. } => {
                                            temperature_manager.update_target_cache("hotend", *temp).await;
                                        }
                                        MCommand::SetBedTemp { temp } => {
                                            temperature_manager.update_target_cache("bed", *temp).await;
                                        }
                                        _ => {}
                                    }
                                }
                                // Update mode tracking for checkpoint
                                match m_cmd {
                                    MCommand::ExtruderAbsoluteMode => e_is_relative = false,
                                    MCommand::ExtruderRelativeMode => e_is_relative = true,
                                    MCommand::SetFeedratePercentage { percentage, .. } => {
                                        feed_rate = *percentage as u16;
                                    }
                                    MCommand::SetFlowPercentage { percentage, .. } => {
                                        flow_rate = *percentage as u16;
                                    }
                                    MCommand::SetFanSpeed { speed, .. } => {
                                        fan_speed = *speed;
                                    }
                                    MCommand::FanOff { .. } => {
                                        fan_speed = 0;
                                    }
                                    _ => {}
                                }
                                // State-change M commands trigger immediate checkpoint save
                                save_immediately = matches!(m_cmd,
                                    MCommand::SetHotendTemp { .. } |
                                    MCommand::SetBedTemp { .. } |
                                    MCommand::WaitHotendTemp { .. } |
                                    MCommand::WaitBedTemp { .. } |
                                    MCommand::ExtruderAbsoluteMode |
                                    MCommand::ExtruderRelativeMode |
                                    MCommand::SetFeedratePercentage { .. } |
                                    MCommand::SetFlowPercentage { .. } |
                                    MCommand::SetFanSpeed { .. } |
                                    MCommand::FanOff { .. }
                                );
                                let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                                let mut progress = self.progress.write().await;
                                progress.percent = percent;
                                progress.current_line = current_line;
                                progress.total_lines = total_lines;
                                progress.status = format!("Executed: {}", line);
                            }
                            Err(e) => {
                                tracing::warn!("M command failed line {}: {} — {}", current_line, line, e);
                                let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                                let mut progress = self.progress.write().await;
                                progress.percent = percent;
                                progress.current_line = current_line;
                                progress.total_lines = total_lines;
                                progress.status = format!("M cmd error: {}", e);
                                return Err(format!("M command failed at line {}: {}", current_line, e));
                            }
                        }
                    }
                }
                _ => {
                    // Empty or Unsupported - just update progress
                    let percent = (valid_processed as f32 / total_valid as f32) * 100.0;
                    let mut progress = self.progress.write().await;
                    progress.percent = percent;
                    progress.current_line = current_line;
                    progress.total_lines = total_lines;
                    progress.status = "Skipped".to_string();
                }
            }
            // Adjust progress for MCU execution buffer lag, then sync.
            // Uses a max 50% correction factor: when buf_time >= 1000ms,
            // displayed progress = 50% of raw line-based progress.
            // Before any 0x1A arrives, buf_time holds u16::MAX (sentinel),
            // which clamps to 1000ms → maximum correction.
            {
                let mut progress = self.progress.write().await;
                let buf_time = mcu_buf_time_ms.load(Ordering::Relaxed);
                // Sentinels and values > 1000ms both map to full correction
                let buf_ratio = ((buf_time as f32).min(1000.0) / 1000.0).max(0.0);
                progress.percent = progress.percent * (1.0 - buf_ratio * 0.5);
            }
            // Checkpoint: save every checkpoint_interval valid commands.
            if let Some(ref cm) = self.checkpoint_manager {
                // Lock briefly to read config (avoids holding lock across await).
                let interval = {
                    let mgr = cm.lock().unwrap();
                    let cfg = mgr.config();
                    if !cfg.enabled {
                        0
                    } else {
                        cfg.checkpoint_interval
                    }
                };
                if interval > 0 && valid_processed > 0 && (save_immediately || valid_processed % interval == 0) {
                    save_immediately = false;
                    let temp = temperature_manager.get_temp_status().await;
                    let ctx = CheckpointContext {
                        file_path: file_path.to_string(),
                        line: current_line,
                        z: position_z,
                        hotend_temp: temp.hotend_target,
                        bed_temp: temp.bed_target,
                        feed_rate,
                        flow_rate,
                        is_absolute,
                        e_is_relative,
                        fan_speed,
                    };
                    let mut mgr = cm.lock().unwrap();
                    if let Err(e) = mgr.save(ctx) {
                        tracing::warn!("Checkpoint save failed at line {}: {}", current_line, e);
                    }
                }
            }
            self.sync_progress_to_device_state().await;
        }

        // All lines dispatched. Wait for MCU to finish executing before marking 100%.
        tracing::info!("All G-code lines dispatched, waiting for MCU to drain buffer...");
        {
            let mut progress = self.progress.write().await;
            progress.status = "Waiting for MCU to finish...".to_string();
        }

        // Use client-side 0x1A buf_time polling to detect MCU drain.
        // The MCU stays in print mode (no ExitPrintMode sent yet), so 0x1A frames
        // continue streaming. This bypasses the complex server-side flow controller
        // drain chain (fc ↔ process_queued_batches ↔ write_task), which has proven
        // unreliable for detecting when ALL motion has completed.
        //
        // The buf_time reflects the MCU's remaining execution time in ms.
        // When it drops to 0 (or near 0), all motion is physically done.
        {
            let drain_timeout = std::time::Duration::from_secs(300);
            let check_interval = std::time::Duration::from_millis(100);
            let drain_start = std::time::Instant::now();

            loop {
                tokio::time::sleep(check_interval).await;
                let buf_time = mcu_buf_time_ms.load(Ordering::Relaxed);

                // Log buf_time every ~5s for debugging
                let elapsed_ms = drain_start.elapsed().as_millis() as u64;
                if elapsed_ms % 5000 < 100 {
                    tracing::info!("Drain polling: buf_time={}ms (elapsed={}s)", buf_time, elapsed_ms / 1000);
                }

                if buf_time <= 10 {
                    // tracing::info!("MCU buffer drained via 0x1A buf_time: buf_time={}ms", buf_time);
                    break;
                }

                if drain_start.elapsed() > drain_timeout {
                    tracing::warn!(
                        "MCU drain timeout after {:.0}s, buf_time={}ms, forcing complete",
                        drain_start.elapsed().as_secs_f64(),
                        buf_time
                    );
                    break;
                }
            }
        }

        // Send ExitPrintMode to MCU before cleaning up the callback,
        // so the MCU knows to stop status reporting and clean up its state.
        tracing::info!("Sending ExitPrintMode to MCU...");
        match client.serial_exit_print_mode().await {
            Ok(_) => tracing::info!("ExitPrintMode acknowledged"),
            Err(e) => tracing::warn!("ExitPrintMode failed: {}", e),
        }

        // Clean up buf_time callback (no more 0x1A frames after ExitPrintMode)
        client.clear_buf_time_callback().await;

        // Mark progress as complete
        let mut progress = self.progress.write().await;
        progress.percent = 100.0;
        progress.current_line = total_lines;
        progress.total_lines = total_lines;
        progress.status = "Completed".to_string();
        // Push final progress to DeviceStateManager
        if let Some(ref ds) = self.device_state {
            ds.update_print_progress(progress.clone()).await;
        }

        // Clear checkpoint on successful completion
        if let Some(ref cm) = self.checkpoint_manager {
            let mut mgr = cm.lock().unwrap();
            if mgr.has_checkpoint() {
                if let Err(e) = mgr.clear() {
                    tracing::warn!("Failed to clear checkpoint on completion: {}", e);
                } else {
                    tracing::info!("Checkpoint cleared (print completed successfully)");
                }
            }
        }

        tracing::info!("Print completed: {}", filename);
        Ok(())
    }
    
    pub async fn load_file(&self, file_path: &str) -> EmbResult<PrintJob> {
        let mut parser = GCodeFileParser::new();
        parser.load_file(file_path)?;
        
        let mut job = PrintJob::new();
        job.file_path = file_path.to_string();
        job.file_name = std::path::Path::new(file_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        
        // Update progress with total lines
        let mut progress = self.progress.write().await;
        progress.total_layers = parser.total_lines();
        
        *self.gcode_parser.write().await = Some(parser);
        *self.current_job.write().await = Some(job.clone());
        
        Ok(job)
    }
    
    pub async fn start(&self) -> EmbResult<()> {
        // Transition to Starting (validates Idle → Starting)
        self.state_machine.write().await.transition_to(PrintState::Starting)
            .map_err(|e| crate::common::EmbError::StateMachine(e))?;

        // Run safety checks before starting
        if let Some(ref safety) = self.safety_controller {
            if safety.has_safety_violation().await {
                return Err(crate::common::EmbError::Safety(
                    "Safety violation detected, cannot start print".to_string()
                ));
            }
        }

        if let Some(ref mut job) = *self.current_job.write().await {
            job.started_at = Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs());
        }

        // Reset progress
        let mut progress = self.progress.write().await;
        progress.percent = 0.0;
        progress.current_layer = 0;
        progress.elapsed_seconds = 0;
        drop(progress);

        // Transition to Printing
        self.state_machine.write().await.transition_to(PrintState::Printing)
            .map_err(|e| crate::common::EmbError::StateMachine(e))?;

        Ok(())
    }
    
    pub async fn pause(&self) -> EmbResult<()> {
        self.state_machine.write().await.transition_to(PrintState::Paused)
            .map_err(|e| crate::common::EmbError::StateMachine(e))?;
        Ok(())
    }
    
    pub async fn resume(&self) -> EmbResult<()> {
        // Validate Paused → Printing transition
        self.state_machine.write().await.transition_to(PrintState::Printing)
            .map_err(|e| crate::common::EmbError::StateMachine(e))?;

        // Run safety checks before resuming
        if let Some(ref safety) = self.safety_controller {
            if safety.has_safety_violation().await {
                return Err(crate::common::EmbError::Safety(
                    "Safety violation detected, cannot resume print".to_string()
                ));
            }
        }

        Ok(())
    }
    
    pub async fn stop(&self) -> EmbResult<()> {
        self.stop_requested.store(true, Ordering::SeqCst);
        self.state_machine.write().await.transition_to(PrintState::Stopping)
            .map_err(|e| crate::common::EmbError::StateMachine(e))?;
        Ok(())
    }
    
    /// Emergency stop (new)
    pub async fn emergency_stop(&self) -> EmbResult<()> {
        if let Some(ref safety) = self.safety_controller {
            safety.handle_emergency_stop().await?;
        }

        self.stop_requested.store(true, Ordering::SeqCst);
        self.state_machine.write().await.transition_to(PrintState::Stopping)
            .map_err(|e| crate::common::EmbError::StateMachine(e))?;

        Ok(())
    }
    
    pub async fn get_state(&self) -> PrintState {
        self.state_machine.read().await.current_state()
    }
    
    pub async fn get_current_job(&self) -> Option<PrintJob> {
        self.current_job.read().await.clone()
    }
    
    /// Get print progress (new)
    pub async fn get_progress(&self) -> PrintProgress {
        self.progress.read().await.clone()
    }
    
    /// Update progress (new) and push to DeviceStateManager
    pub async fn update_progress(&self, percent: f32, layer: u32) {
        let mut progress = self.progress.write().await;
        progress.percent = percent;
        progress.current_layer = layer;
        
        // Calculate elapsed and remaining time
        if let Some(ref job) = self.current_job.read().await.as_ref() {
            if let Some(started_at) = job.started_at {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                progress.elapsed_seconds = now - started_at;
                
                if percent > 0.0 {
                    let total_estimated = job.estimated_time_seconds;
                    progress.remaining_seconds = 
                        ((total_estimated as f32 * (100.0 - percent) / percent) as u64)
                        .max(0);
                }
            }
        }
        
        // Push to DeviceStateManager
        if let Some(ref ds) = self.device_state {
            ds.update_print_progress(progress.clone()).await;
        }
    }
    
    /// Sync current print progress to DeviceStateManager.
    /// Call after any direct self.progress writes (e.g. in execute_print_loop).
    pub async fn sync_progress_to_device_state(&self) {
        if let Some(ref ds) = self.device_state {
            ds.update_print_progress(self.progress.read().await.clone()).await;
        }
    }
    
    /// Get temperature presets (new)
    pub async fn get_presets(&self) -> Vec<TemperaturePreset> {
        self.presets.read().await.clone()
    }
    
    /// Add temperature preset (new)
    pub async fn add_preset(&self, preset: TemperaturePreset) {
        self.presets.write().await.push(preset);
    }
    
    /// Apply temperature preset (new)
    pub async fn apply_preset(&self, preset_name: &str) -> EmbResult<()> {
        let preset = {
            let presets = self.presets.read().await;
            presets.iter()
                .find(|p| p.name == preset_name)
                .cloned()
                .ok_or_else(|| crate::common::EmbError::Config(
                    format!("Temperature preset '{}' not found", preset_name)
                ))?
        };

        let client = self.client.as_ref()
            .ok_or_else(|| crate::common::EmbError::Config(
                "CoreSocketClient not set".to_string()
            ))?;

        let temperature_manager = self.temperature_manager.as_ref()
            .ok_or_else(|| crate::common::EmbError::Config(
                "TemperatureManager not set".to_string()
            ))?;

        tracing::info!("Applying temperature preset: {} (hotend={}, bed={})",
            preset.name, preset.hotend_temp, preset.bed_temp);

        // Set hotend temperature (M104)
        if preset.hotend_temp > 0.0 {
            client.motion_execute_m_command(emb_api::MCommand::SetHotendTemp {
                tool: 0,
                temp: preset.hotend_temp,
            }).await?;
            temperature_manager.update_target_cache("hotend", preset.hotend_temp).await;
        }

        // Set bed temperature (M140)
        if preset.bed_temp > 0.0 {
            client.motion_execute_m_command(emb_api::MCommand::SetBedTemp {
                temp: preset.bed_temp,
            }).await?;
            temperature_manager.update_target_cache("bed", preset.bed_temp).await;
        }

        Ok(())
    }
    
    /// Get device state manager (new)
    pub fn device_state(&self) -> Option<&Arc<DeviceStateManager>> {
        self.device_state.as_ref()
    }
    
    /// Get safety controller (new)
    pub fn safety_controller(&self) -> Option<&Arc<SafetyController>> {
        self.safety_controller.as_ref()
    }
}

/// Extract the last E (extruder) parameter value from a G-code line.
/// Handles lines like: `G1 X10 Y20 Z0.3 E1.234` or `G92 E12.5`
fn extract_e_value(line: &str) -> Option<f32> {
    let trimmed = line.trim();
    // Skip empty lines and comments
    if trimmed.is_empty() || trimmed.starts_with(';') {
        return None;
    }
    // Remove inline comment
    let code = if let Some(pos) = trimmed.find(';') {
        &trimmed[..pos]
    } else {
        trimmed
    };
    let code = code.trim();
    if code.is_empty() {
        return None;
    }
    // Scan for an 'E' parameter: find "E" followed by a number
    // Split by whitespace and look for tokens starting with E or e
    for token in code.split_whitespace() {
        if token.len() < 2 {
            continue;
        }
        let upper = token.to_uppercase();
        if upper.starts_with('E') {
            if let Ok(val) = upper[1..].parse::<f32>() {
                return Some(val);
            }
        }
    }
    None
}

impl Default for PrintController {
    fn default() -> Self {
        Self::new()
    }
}


