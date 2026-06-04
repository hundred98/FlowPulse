//! Message types for communication channels
//!
//! This module defines unified message types for WebSocket, UnixSocket, and SharedMemory communication.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// WebSocket message type (used for WebSocket and UnixSocket)
/// Follows the frontend architecture specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WebSocketMessage {
    /// Temperature update
    Temperature {
        hotend_current: f32,
        hotend_target: f32,
        bed_current: f32,
        bed_target: f32,
    },
    
    /// Position update
    Position {
        x: f32,
        y: f32,
        z: f32,
        e: f32,
    },
    
    /// Progress update
    Progress {
        percent: f32,
        current_layer: u32,
        total_layers: u32,
    },
    
    /// State transition
    State {
        from: String,
        to: String,
    },
    
    /// Print event
    PrintEvent {
        event: String,
        message: String,
    },
    
    /// Alert message
    Alert {
        severity: String,
        message: String,
    },
    
    /// Limit switch status
    LimitSwitch {
        x: bool,
        y: bool,
        z: bool,
    },
    
    /// Homing status
    Homing {
        axis: String,
        status: String,
        progress: f32,
    },
}

/// Printer status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterStatus {
    /// Current printer state
    pub state: String,
    
    /// Print progress percentage
    pub progress_percent: f32,
    
    /// Current layer number
    pub current_layer: u32,
    
    /// Total layer count
    pub total_layers: u32,
    
    /// Elapsed time in seconds
    pub elapsed_seconds: u64,
    
    /// Estimated remaining time in seconds
    pub remaining_seconds: u64,
    
    /// Current print file name
    pub print_file: Option<String>,
}

/// Temperature status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempStatus {
    /// Hotend current temperature
    pub hotend_current: f32,
    
    /// Hotend target temperature
    pub hotend_target: f32,
    
    /// Bed current temperature
    pub bed_current: f32,
    
    /// Bed target temperature
    pub bed_target: f32,
    
    /// Additional heater temperatures (key: heater name, value: (current, target))
    pub additional_heaters: HashMap<String, (f32, f32)>,
}

/// Position data
#[derive(Debug, Clone, Serialize, Deserialize, Copy)]
pub struct PositionData {
    /// X axis position
    pub x: f32,
    
    /// Y axis position
    pub y: f32,
    
    /// Z axis position
    pub z: f32,
    
    /// E axis position (extruder)
    pub e: f32,
}

/// Shared memory state (for high-performance scenarios)
/// Reserved for future implementation
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SharedState {
    /// Printer state (0-255)
    pub printer_state: u8,
    
    /// Position X
    pub position_x: f32,
    
    /// Position Y
    pub position_y: f32,
    
    /// Position Z
    pub position_z: f32,
    
    /// Position E
    pub position_e: f32,
    
    /// Hotend current temperature
    pub hotend_current: f32,
    
    /// Hotend target temperature
    pub hotend_target: f32,
    
    /// Bed current temperature
    pub bed_current: f32,
    
    /// Bed target temperature
    pub bed_target: f32,
    
    /// Progress percentage
    pub progress_percent: f32,
    
    /// Current layer
    pub current_layer: u32,
    
    /// Total layers
    pub total_layers: u32,
    
    /// Update flag (atomic bool for synchronization)
    pub update_flag: bool,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            printer_state: 0,
            position_x: 0.0,
            position_y: 0.0,
            position_z: 0.0,
            position_e: 0.0,
            hotend_current: 0.0,
            hotend_target: 0.0,
            bed_current: 0.0,
            bed_target: 0.0,
            progress_percent: 0.0,
            current_layer: 0,
            total_layers: 0,
            update_flag: false,
        }
    }
}

impl TempStatus {
    /// Create a new temperature status
    pub fn new(hotend_current: f32, hotend_target: f32, bed_current: f32, bed_target: f32) -> Self {
        Self {
            hotend_current,
            hotend_target,
            bed_current,
            bed_target,
            additional_heaters: HashMap::new(),
        }
    }
    
    /// Add an additional heater
    pub fn add_heater(&mut self, name: String, current: f32, target: f32) {
        self.additional_heaters.insert(name, (current, target));
    }
}

impl PositionData {
    /// Create a new position data
    pub fn new(x: f32, y: f32, z: f32, e: f32) -> Self {
        Self { x, y, z, e }
    }
    
    /// Create zero position
    pub fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

impl PrinterStatus {
    /// Create a new printer status
    pub fn new(state: String) -> Self {
        Self {
            state,
            progress_percent: 0.0,
            current_layer: 0,
            total_layers: 0,
            elapsed_seconds: 0,
            remaining_seconds: 0,
            print_file: None,
        }
    }
    
    /// Create idle status
    pub fn idle() -> Self {
        Self::new("idle".to_string())
    }
}