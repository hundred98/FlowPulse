//! Device state manager
//!
//! This module manages device state synchronization and caching,
//! providing unified access to device status from the core server.

use crate::common::{
    EmbResult, EventPublisher, PrinterStatus, PositionData,
    PrinterEvent, EventKind, EventSeverity,
};
use crate::core_client::CoreSocketClient;
use std::sync::Arc;
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
        // Get position from core server
        if let Ok((x, y, z, e)) = self.client.motion_get_position().await {
            let pos = Position { x, y, z, e };
            self.update_position(pos).await;
        }
        
        // Get motion stats from core server
        if let Ok(stats) = self.client.motion_query_stats().await {
            // Determine motion status based on stats
            let motion_status = if stats.motion.total_steps > 0 {
                MotionStatus::Printing
            } else {
                MotionStatus::Idle
            };
            self.update_motion_status(motion_status).await;
            
            // Update flow status based on stats
            let flow_status = FlowStatus {
                flow_rate: stats.motion.avg_speed_mm_per_s as f32,
                pressure: 0.0, // Not available from stats
                is_active: stats.motion.total_steps > 0,
            };
            self.update_flow_status(flow_status).await;
        }
        
        // Update last sync time
        let mut last_sync = self.last_sync.write().await;
        *last_sync = Instant::now();
        
        // Save snapshot to history
        self.save_snapshot().await;
        
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
    
    /// Update flow status
    pub async fn update_flow_status(&self, status: FlowStatus) {
        let mut flow_status = self.flow_status.write().await;
        *flow_status = status;
        
        // Publish flow status update event
        let _ = self.event_publisher.publish(PrinterEvent::new(
            EventKind::StateChanged,
            "device_state".to_string(),
            format!("Flow status updated: rate={}, active={}", status.flow_rate, status.is_active),
        ).with_severity(EventSeverity::Info));
    }

    /// Take a state snapshot
    pub async fn take_snapshot(&self) -> DeviceStateSnapshot {
        let position = self.position.read().await.clone();
        let motion_status = self.motion_status.read().await.clone();
        let flow_status = self.flow_status.read().await.clone();

        DeviceStateSnapshot {
            timestamp: Utc::now(),
            position,
            motion_status,
            flow_status,
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