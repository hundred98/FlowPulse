use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::gcode::GCodeFileParser;
use crate::common::EmbResult;
use crate::state::DeviceStateManager;
use crate::safety::SafetyController;
use crate::temperature::TemperaturePreset;
use crate::core_client::CoreSocketClient;
use crate::temperature::TemperatureManager;
use crate::gcode::{GCodeParser, CommandKind, MotionCommand};
use emb_api::{MExecutionType, MCommand, ArcParamsApi};
use super::state_machine::PrintStateMachine;

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
    pub async fn execute_print_loop(&self, filename: &str, content: &str, total_lines: usize) -> Result<(), String> {
        let client = self.client.as_ref().ok_or("PrintController: CoreSocketClient not set")?;
        let temperature_manager = self.temperature_manager.as_ref().ok_or("PrintController: TemperatureManager not set")?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = total_lines.max(lines.len());

        tracing::info!("Starting print execution: {} ({} lines)", filename, total_lines);

        for (line_idx, line) in lines.iter().enumerate() {
            // Check for stop request
            if self.stop_requested.load(Ordering::SeqCst) {
                tracing::info!("Print stopped by user request at line {}", line_idx + 1);
                return Err("Print stopped by user".to_string());
            }

            let line = line.trim();

            // Skip empty and comment lines
            if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
                let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                let mut progress = self.progress.write().await;
                progress.percent = percent;
                progress.current_line = (line_idx + 1) as u32;
                progress.total_lines = total_lines as u32;
                progress.status = format!("Skipping: {}", line);
                self.sync_progress_to_device_state().await;
                continue;
            }

            // Parse the line
            let cmd = match GCodeParser::parse_line(line, line_idx as u32) {
                Some(parsed) => parsed,
                None => {
                    tracing::warn!("Parse error line {}: {}", line_idx + 1, line);
                    let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                    let mut progress = self.progress.write().await;
                    progress.percent = percent;
                    progress.current_line = (line_idx + 1) as u32;
                    progress.total_lines = total_lines as u32;
                    progress.status = format!("Parse error: {}", line);
                    self.sync_progress_to_device_state().await;
                    continue;
                }
            };

            match &cmd.kind {
                CommandKind::Motion(motion_cmd) => {
                    match motion_cmd {
                        MotionCommand::LinearMove { x, y, z, e, f, is_rapid } => {
                            let cmd_str = if *is_rapid { "G0" } else { "G1" };
                            match client.motion_dispatch_arc(cmd_str, *x, *y, *z, *e, *f, None).await {
                                Ok(_) => {
                                    let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                                    let mut progress = self.progress.write().await;
                                    progress.percent = percent;
                                    progress.current_line = (line_idx + 1) as u32;
                                    progress.total_lines = total_lines as u32;
                                    progress.status = format!("Dispatched: {}", line);
                                }
                                Err(e) => {
                                    tracing::warn!("Motion dispatch failed line {}: {}", line_idx + 1, e);
                                    let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                                    let mut progress = self.progress.write().await;
                                    progress.percent = percent;
                                    progress.current_line = (line_idx + 1) as u32;
                                    progress.total_lines = total_lines as u32;
                                    progress.status = format!("Motion error: {}", e);
                                    return Err(format!("Motion dispatch failed at line {}: {}", line_idx + 1, e));
                                }
                            }
                        }
                        MotionCommand::ArcMove { x, y, z, e, f, i, j, is_cw } => {
                            let cmd_str = if *is_cw { "G2" } else { "G3" };
                            let arc = Some(ArcParamsApi {
                                i: *i,
                                j: *j,
                                direction: if *is_cw { 0 } else { 1 },
                            });
                            match client.motion_dispatch_arc(cmd_str, *x, *y, *z, *e, *f, arc).await {
                                Ok(_) => {
                                    let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                                    let mut progress = self.progress.write().await;
                                    progress.percent = percent;
                                    progress.current_line = (line_idx + 1) as u32;
                                    progress.total_lines = total_lines as u32;
                                    progress.status = format!("Dispatched: {}", line);
                                }
                                Err(e) => {
                                    tracing::warn!("Arc dispatch failed line {}: {}", line_idx + 1, e);
                                    let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                                    let mut progress = self.progress.write().await;
                                    progress.percent = percent;
                                    progress.current_line = (line_idx + 1) as u32;
                                    progress.total_lines = total_lines as u32;
                                    progress.status = format!("Arc error: {}", e);
                                    return Err(format!("Arc dispatch failed at line {}: {}", line_idx + 1, e));
                                }
                            }
                        }
                        MotionCommand::Dwell { dwell_time_ms } => {
                            match client.motion_dwell(*dwell_time_ms).await {
                                Ok(()) => {
                                    let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                                    let mut progress = self.progress.write().await;
                                    progress.percent = percent;
                                    progress.current_line = (line_idx + 1) as u32;
                                    progress.total_lines = total_lines as u32;
                                    progress.status = format!("Dwell: {}ms", dwell_time_ms);
                                }
                                Err(e) => {
                                    tracing::warn!("Dwell failed line {}: {}", line_idx + 1, e);
                                    return Err(format!("Dwell failed at line {}: {}", line_idx + 1, e));
                                }
                            }
                        }
                        _ => {
                            tracing::info!("  → Skipping non-motion command: {:?}", motion_cmd);
                            let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                            let mut progress = self.progress.write().await;
                            progress.percent = percent;
                            progress.current_line = (line_idx + 1) as u32;
                            progress.total_lines = total_lines as u32;
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
                                        let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                                        let mut progress = self.progress.write().await;
                                        progress.percent = percent;
                                        progress.current_line = (line_idx + 1) as u32;
                                        progress.total_lines = total_lines as u32;
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

                                let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                                let mut progress = self.progress.write().await;
                                progress.percent = percent;
                                progress.current_line = (line_idx + 1) as u32;
                                progress.total_lines = total_lines as u32;
                                progress.status = format!("Temp OK: {} = {}°C", heater, target);
                            }
                            Err(e) => {
                                tracing::warn!("M command failed line {}: {} — {}", line_idx + 1, line, e);
                                let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                                let mut progress = self.progress.write().await;
                                progress.percent = percent;
                                progress.current_line = (line_idx + 1) as u32;
                                progress.total_lines = total_lines as u32;
                                progress.status = format!("M cmd error: {}", e);
                                return Err(format!("M command failed at line {}: {}", line_idx + 1, e));
                            }
                        }
                    } else if exec_type == MExecutionType::Query {
                        // M105: read from cache only
                        let heaters = temperature_manager.get_all_heaters().await;
                        tracing::info!("  → Temperature query (cached):");
                        for (name, s) in &heaters {
                            tracing::info!("      {}: {:.1}°C / {:.1}°C", name, s.current_temp, s.target_temp);
                        }
                        let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                        let mut progress = self.progress.write().await;
                        progress.percent = percent;
                        progress.current_line = (line_idx + 1) as u32;
                        progress.total_lines = total_lines as u32;
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
                                let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                                let mut progress = self.progress.write().await;
                                progress.percent = percent;
                                progress.current_line = (line_idx + 1) as u32;
                                progress.total_lines = total_lines as u32;
                                progress.status = format!("Executed: {}", line);
                            }
                            Err(e) => {
                                tracing::warn!("M command failed line {}: {} — {}", line_idx + 1, line, e);
                                let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                                let mut progress = self.progress.write().await;
                                progress.percent = percent;
                                progress.current_line = (line_idx + 1) as u32;
                                progress.total_lines = total_lines as u32;
                                progress.status = format!("M cmd error: {}", e);
                                return Err(format!("M command failed at line {}: {}", line_idx + 1, e));
                            }
                        }
                    }
                }
                _ => {
                    // Empty or Unsupported - just update progress
                    let percent = ((line_idx + 1) as f32 / total_lines as f32) * 100.0;
                    let mut progress = self.progress.write().await;
                    progress.percent = percent;
                    progress.current_line = (line_idx + 1) as u32;
                    progress.total_lines = total_lines as u32;
                    progress.status = "Skipped".to_string();
                }
            }
            // Sync progress to DeviceStateManager after each line
            self.sync_progress_to_device_state().await;
        }

        // Mark progress as complete
        let mut progress = self.progress.write().await;
        progress.percent = 100.0;
        progress.current_line = total_lines as u32;
        progress.total_lines = total_lines as u32;
        progress.status = "Completed".to_string();
        // Push final progress to DeviceStateManager
        if let Some(ref ds) = self.device_state {
            ds.update_print_progress(progress.clone()).await;
        }

        tracing::info!("✅ Print completed: {}", filename);
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

impl Default for PrintController {
    fn default() -> Self {
        Self::new()
    }
}


