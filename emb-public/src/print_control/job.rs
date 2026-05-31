use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

use crate::gcode::GCodeFileParser;
use crate::common::EmbResult;

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

pub struct PrintController {
    state: Arc<RwLock<PrintState>>,
    current_job: Arc<RwLock<Option<PrintJob>>>,
    gcode_parser: Arc<RwLock<Option<GCodeFileParser>>>,
    #[allow(dead_code)]
    presets: Arc<RwLock<Vec<TemperaturePreset>>>,
    #[allow(dead_code)]
    motion_config: Arc<RwLock<MotionConfig>>,
    #[allow(dead_code)]
    safety_config: Arc<RwLock<SafetyConfig>>,
    stop_requested: Arc<AtomicBool>,
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
        }
    }
    
    pub async fn load_file(&self, file_path: &str) -> EmbResult<PrintJob> {
        let parser = GCodeFileParser::new(file_path)?;
        let mut job = PrintJob::new();
        job.file_path = file_path.to_string();
        job.file_name = std::path::Path::new(file_path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        
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
        
        *self.state.write().await = PrintState::Starting;
        
        if let Some(ref mut job) = *self.current_job.write().await {
            job.started_at = Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs());
        }
        
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
        
        *self.state.write().await = PrintState::Printing;
        Ok(())
    }
    
    pub async fn stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
        *self.state.write().await = PrintState::Stopping;
    }
    
    pub async fn get_state(&self) -> PrintState {
        *self.state.read().await
    }
    
    pub async fn get_current_job(&self) -> Option<PrintJob> {
        self.current_job.read().await.clone()
    }
}

impl Default for PrintController {
    fn default() -> Self {
        Self::new()
    }
}
