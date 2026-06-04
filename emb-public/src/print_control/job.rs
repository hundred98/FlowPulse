use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::gcode::GCodeFileParser;
use crate::common::EmbResult;
use crate::state::DeviceStateManager;
use crate::safety::SafetyController;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperaturePreset {
    pub name: String,
    pub hotend_temp: f32,
    pub bed_temp: f32,
    pub fan_speed: u8,
}

impl Default for TemperaturePreset {
    fn default() -> Self {
        Self {
            name: "PLA".to_string(),
            hotend_temp: 200.0,
            bed_temp: 60.0,
            fan_speed: 100,
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
}

impl Default for PrintProgress {
    fn default() -> Self {
        Self {
            percent: 0.0,
            current_layer: 0,
            total_layers: 0,
            elapsed_seconds: 0,
            remaining_seconds: 0,
        }
    }
}

pub struct PrintController {
    state: Arc<RwLock<PrintState>>,
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
}

impl PrintController {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(PrintState::Idle)),
            current_job: Arc::new(RwLock::new(None)),
            gcode_parser: Arc::new(RwLock::new(None)),
            presets: Arc::new(RwLock::new(vec![
                TemperaturePreset::default(),
                TemperaturePreset {
                    name: "ABS".to_string(),
                    hotend_temp: 240.0,
                    bed_temp: 100.0,
                    fan_speed: 0,
                },
            ])),
            motion_config: Arc::new(RwLock::new(MotionConfig::default())),
            safety_config: Arc::new(RwLock::new(SafetyConfig::default())),
            stop_requested: Arc::new(AtomicBool::new(false)),
            device_state: None,
            safety_controller: None,
            progress: Arc::new(RwLock::new(PrintProgress::default())),
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
        let state = *self.state.read().await;
        if state != PrintState::Idle {
            return Err(crate::common::EmbError::StateMachine(
                format!("Cannot start from state {:?}", state)
            ));
        }
        
        // New: Run safety checks before starting
        if let Some(ref safety) = self.safety_controller {
            if safety.has_safety_violation().await {
                return Err(crate::common::EmbError::Safety(
                    "Safety violation detected, cannot start print".to_string()
                ));
            }
        }
        
        *self.state.write().await = PrintState::Starting;
        
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
        
        *self.state.write().await = PrintState::Printing;
        
        Ok(())
    }
    
    pub async fn pause(&self) -> EmbResult<()> {
        let state = *self.state.read().await;
        if state != PrintState::Printing {
            return Err(crate::common::EmbError::StateMachine(
                format!("Cannot pause from state {:?}", state)
            ));
        }
        
        *self.state.write().await = PrintState::Paused;
        Ok(())
    }
    
    pub async fn resume(&self) -> EmbResult<()> {
        let state = *self.state.read().await;
        if state != PrintState::Paused {
            return Err(crate::common::EmbError::StateMachine(
                format!("Cannot resume from state {:?}", state)
            ));
        }
        
        // New: Run safety checks before resuming
        if let Some(ref safety) = self.safety_controller {
            if safety.has_safety_violation().await {
                return Err(crate::common::EmbError::Safety(
                    "Safety violation detected, cannot resume print".to_string()
                ));
            }
        }
        
        *self.state.write().await = PrintState::Printing;
        Ok(())
    }
    
    pub async fn stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
        *self.state.write().await = PrintState::Stopping;
    }
    
    /// Emergency stop (new)
    pub async fn emergency_stop(&self) -> EmbResult<()> {
        if let Some(ref safety) = self.safety_controller {
            safety.handle_emergency_stop().await?;
        }
        
        self.stop_requested.store(true, Ordering::SeqCst);
        *self.state.write().await = PrintState::Stopping;
        
        Ok(())
    }
    
    pub async fn get_state(&self) -> PrintState {
        *self.state.read().await
    }
    
    pub async fn get_current_job(&self) -> Option<PrintJob> {
        self.current_job.read().await.clone()
    }
    
    /// Get print progress (new)
    pub async fn get_progress(&self) -> PrintProgress {
        self.progress.read().await.clone()
    }
    
    /// Update progress (new)
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
        let presets = self.presets.read().await;
        let preset = presets.iter()
            .find(|p| p.name == preset_name)
            .cloned();
        
        if preset.is_none() {
            return Err(crate::common::EmbError::Config(
                format!("Temperature preset '{}' not found", preset_name)
            ));
        }
        
        let preset = preset.unwrap();
        
        // TODO: Send temperature commands to core server
        log::info!("Applying temperature preset: {} (hotend={}, bed={})", 
            preset.name, preset.hotend_temp, preset.bed_temp);
        
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
