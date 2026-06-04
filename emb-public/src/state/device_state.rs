//! Device state manager
//!
//! This module manages device state synchronization and caching,
//! providing unified access to device status from the core server.

use crate::common::{
    EmbResult, EventPublisher, PrinterStatus, TempStatus, PositionData,
    PrinterEvent, EventKind, EventSeverity,
};
use crate::core_client::CoreSocketClient;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

/// Device state manager configuration
#[derive(Debug, Clone)]
pub struct DeviceStateConfig {
    /// State synchronization interval (milliseconds)
    pub sync_interval_ms: u64,
    
    /// Enable state caching
    pub enable_cache: bool,
    
    /// State history size
    pub history_size: usize,
}

impl Default for DeviceStateConfig {
    fn default() -> Self {
        Self {
            sync_interval_ms: 1000,
            enable_cache: true,
            history_size: 100,
        }
    }
}

/// Position data from device
#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub e: f32,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            e: 0.0,
        }
    }
}

/// Motion status from device
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MotionStatus {
    Idle,
    Moving,
    Homing,
    Printing,
    Error,
}

impl Default for MotionStatus {
    fn default() -> Self {
        Self::Idle
    }
}

/// Flow status from device
#[derive(Debug, Clone, Copy)]
pub struct FlowStatus {
    pub flow_rate: f32,
    pub pressure: f32,
    pub is_active: bool,
}

impl Default for FlowStatus {
    fn default() -> Self {
        Self {
            flow_rate: 0.0,
            pressure: 0.0,
            is_active: false,
        }
    }
}

/// Device state manager
/// Synchronizes device state from the core server
pub struct DeviceStateManager {
    /// Core socket client
    #[allow(dead_code)]
    client: Arc<CoreSocketClient>,
    
    /// Event publisher
    event_publisher: Arc<dyn EventPublisher>,
    
    /// Configuration
    config: DeviceStateConfig,
    
    /// Cached position
    position: Arc<RwLock<Position>>,
    
    /// Cached motion status
    motion_status: Arc<RwLock<MotionStatus>>,
    
    /// Cached flow status
    flow_status: Arc<RwLock<FlowStatus>>,
    
    /// Cached temperatures
    temperatures: Arc<RwLock<HashMap<String, f32>>>,
    
    /// Last synchronization time
    last_sync: Arc<RwLock<Instant>>,
    
    /// State history
    history: Arc<RwLock<Vec<DeviceStateSnapshot>>>,
}

/// Device state snapshot
#[derive(Debug, Clone)]
pub struct DeviceStateSnapshot {
    /// Snapshot timestamp
    pub timestamp: DateTime<Utc>,
    
    /// Position at snapshot
    pub position: Position,
    
    /// Motion status at snapshot
    pub motion_status: MotionStatus,
    
    /// Flow status at snapshot
    pub flow_status: FlowStatus,
    
    /// Temperatures at snapshot
    pub temperatures: HashMap<String, f32>,
}

impl DeviceStateManager {
    /// Create a new device state manager
    pub fn new(
        client: Arc<CoreSocketClient>,
        event_publisher: Arc<dyn EventPublisher>,
        config: DeviceStateConfig,
    ) -> Self {
        Self {
            client,
            event_publisher,
            config,
            position: Arc::new(RwLock::new(Position::default())),
            motion_status: Arc::new(RwLock::new(MotionStatus::default())),
            flow_status: Arc::new(RwLock::new(FlowStatus::default())),
            temperatures: Arc::new(RwLock::new(HashMap::new())),
            last_sync: Arc::new(RwLock::new(Instant::now())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Start state synchronization loop
    pub async fn start_sync_loop(&self) {
        let interval = Duration::from_millis(self.config.sync_interval_ms);
        
        loop {
            // Sync state from core server
            if let Err(e) = self.sync_state().await {
                log::error!("Failed to sync device state: {}", e);
            }
            
            // Wait for next sync interval
            tokio::time::sleep(interval).await;
        }
    }
    
    /// Sync state from core server
    pub async fn sync_state(&self) -> EmbResult<()> {
        // TODO: Implement actual state synchronization with core server
        // For now, we simulate state updates
        
        // Update last sync time
        let mut last_sync = self.last_sync.write().await;
        *last_sync = Instant::now();
        
        // Publish sync event
        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "device_state".to_string(),
            "Device state synchronized".to_string(),
        ).with_severity(EventSeverity::Info));
        
        Ok(())
    }
    
    /// Get current position
    pub async fn get_position(&self) -> Position {
        self.position.read().await.clone()
    }
    
    /// Get current motion status
    pub async fn get_motion_status(&self) -> MotionStatus {
        self.motion_status.read().await.clone()
    }
    
    /// Get current flow status
    pub async fn get_flow_status(&self) -> FlowStatus {
        self.flow_status.read().await.clone()
    }
    
    /// Get current temperatures
    pub async fn get_temperatures(&self) -> HashMap<String, f32> {
        self.temperatures.read().await.clone()
    }
    
    /// Update position (called by sync loop or event handler)
    pub async fn update_position(&self, pos: Position) {
        let mut position = self.position.write().await;
        *position = pos;
        
        // Publish position update event
        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::PositionUpdate,
            "device_state".to_string(),
            format!("Position updated: X={}, Y={}, Z={}, E={}", pos.x, pos.y, pos.z, pos.e),
        ).with_severity(EventSeverity::Info));
    }
    
    /// Update motion status
    pub async fn update_motion_status(&self, status: MotionStatus) {
        let mut motion_status = self.motion_status.write().await;
        *motion_status = status;
        
        // Publish motion status update event
        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "device_state".to_string(),
            format!("Motion status updated: {:?}", status),
        ).with_severity(EventSeverity::Info));
    }
    
    /// Update temperature
    pub async fn update_temperature(&self, heater: String, temp: f32) {
        let mut temperatures = self.temperatures.write().await;
        temperatures.insert(heater.clone(), temp);
        
        // Publish temperature update event
        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::TemperatureUpdate,
            "device_state".to_string(),
            format!("Temperature updated: {}={}", heater, temp),
        ).with_severity(EventSeverity::Info));
    }
    
    /// Take a state snapshot
    pub async fn take_snapshot(&self) -> DeviceStateSnapshot {
        let position = self.position.read().await.clone();
        let motion_status = self.motion_status.read().await.clone();
        let flow_status = self.flow_status.read().await.clone();
        let temperatures = self.temperatures.read().await.clone();
        
        DeviceStateSnapshot {
            timestamp: Utc::now(),
            position,
            motion_status,
            flow_status,
            temperatures,
        }
    }
    
    /// Save snapshot to history
    pub async fn save_snapshot(&self) {
        if self.config.history_size == 0 {
            return;
        }
        
        let snapshot = self.take_snapshot().await;
        let mut history = self.history.write().await;
        
        // Add snapshot to history
        history.push(snapshot);
        
        // Trim history if exceeds size limit
        if history.len() > self.config.history_size {
            history.remove(0);
        }
    }
    
    /// Get state history
    pub async fn get_history(&self) -> Vec<DeviceStateSnapshot> {
        self.history.read().await.clone()
    }
    
    /// Get printer status (for FrontendDataProvider)
    pub async fn get_printer_status(&self) -> PrinterStatus {
        let motion_status = self.motion_status.read().await;
        let state = match *motion_status {
            MotionStatus::Idle => "idle",
            MotionStatus::Moving => "moving",
            MotionStatus::Homing => "homing",
            MotionStatus::Printing => "printing",
            MotionStatus::Error => "error",
        };
        
        PrinterStatus::new(state.to_string())
    }
    
    /// Get temperature status (for FrontendDataProvider)
    pub async fn get_temp_status(&self) -> TempStatus {
        let temperatures = self.temperatures.read().await;
        
        let hotend_current = temperatures.get("hotend").copied().unwrap_or(0.0);
        let hotend_target = temperatures.get("hotend_target").copied().unwrap_or(0.0);
        let bed_current = temperatures.get("bed").copied().unwrap_or(0.0);
        let bed_target = temperatures.get("bed_target").copied().unwrap_or(0.0);
        
        TempStatus::new(hotend_current, hotend_target, bed_current, bed_target)
    }
    
    /// Get position data (for FrontendDataProvider)
    pub async fn get_position_data(&self) -> PositionData {
        let position = self.position.read().await;
        PositionData::new(position.x, position.y, position.z, position.e)
    }
    
    /// Get last sync time
    pub async fn get_last_sync_time(&self) -> Instant {
        self.last_sync.read().await.clone()
    }
    
    /// Check if state is stale (not synced recently)
    pub async fn is_stale(&self, threshold_ms: u64) -> bool {
        let last_sync = self.last_sync.read().await;
        let elapsed = last_sync.elapsed().as_millis() as u64;
        elapsed > threshold_ms
    }
}