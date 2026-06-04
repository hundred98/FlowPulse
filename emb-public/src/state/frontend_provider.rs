//! Frontend data provider interface
//!
//! This module defines the unified interface for frontend data access,
//! supporting UnixSocket (priority), SharedMemory (reserved), and WebSocket.

use crate::common::{EmbResult, PrinterStatus, TempStatus, PositionData, SharedState};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

/// Frontend data provider trait
/// Unified interface for different communication methods
pub trait FrontendDataProvider: Send + Sync {
    /// Get current printer status
    fn get_printer_status(&self) -> PrinterStatus;
    
    /// Get current temperature status
    fn get_temperature(&self) -> TempStatus;
    
    /// Get current position data
    fn get_position(&self) -> PositionData;
    
    /// Send G-code command
    fn send_gcode(&self, cmd: &str) -> EmbResult<()>;
}

/// UnixSocket data provider (priority implementation)
/// Simple, reliable, and easy to debug
pub struct UnixSocketProvider {
    /// UnixSocket path
    socket_path: String,
    
    /// Cached printer status
    cached_status: Arc<RwLock<PrinterStatus>>,
    
    /// Cached temperature status
    cached_temp: Arc<RwLock<TempStatus>>,
    
    /// Cached position data
    cached_position: Arc<RwLock<PositionData>>,
}

impl UnixSocketProvider {
    /// Create a new UnixSocket provider
    pub fn new(socket_path: String) -> Self {
        Self {
            socket_path,
            cached_status: Arc::new(RwLock::new(PrinterStatus::idle())),
            cached_temp: Arc::new(RwLock::new(TempStatus::new(0.0, 0.0, 0.0, 0.0))),
            cached_position: Arc::new(RwLock::new(PositionData::zero())),
        }
    }
    
    /// Update cached status
    pub async fn update_status(&self, status: PrinterStatus) {
        let mut cached = self.cached_status.write().await;
        *cached = status;
    }
    
    /// Update cached temperature
    pub async fn update_temperature(&self, temp: TempStatus) {
        let mut cached = self.cached_temp.write().await;
        *cached = temp;
    }
    
    /// Update cached position
    pub async fn update_position(&self, position: PositionData) {
        let mut cached = self.cached_position.write().await;
        *cached = position;
    }
    
    /// Get socket path
    pub fn socket_path(&self) -> &str {
        &self.socket_path
    }
}

impl FrontendDataProvider for UnixSocketProvider {
    fn get_printer_status(&self) -> PrinterStatus {
        // For synchronous trait, we use blocking read
        // In async context, use update_status() to update cache
        self.cached_status.blocking_read().clone()
    }
    
    fn get_temperature(&self) -> TempStatus {
        self.cached_temp.blocking_read().clone()
    }
    
    fn get_position(&self) -> PositionData {
        self.cached_position.blocking_read().clone()
    }
    
    fn send_gcode(&self, cmd: &str) -> EmbResult<()> {
        // TODO: Implement UnixSocket communication
        // For now, return success
        log::info!("UnixSocket: Sending G-code: {}", cmd);
        Ok(())
    }
}

/// Embedded data provider (reserved for shared memory)
/// High-performance scenario (>60fps)
/// Interface reserved, not implemented yet
pub struct EmbeddedDataProvider {
    /// Shared memory state (reserved)
    shared_mem: Arc<RwLock<SharedState>>,
}

impl EmbeddedDataProvider {
    /// Create a new embedded data provider
    /// Reserved for future implementation
    pub fn new() -> Self {
        Self {
            shared_mem: Arc::new(RwLock::new(SharedState::default())),
        }
    }
    
    /// Update shared memory state (reserved)
    pub async fn update_state(&self, state: SharedState) {
        let mut shared = self.shared_mem.write().await;
        *shared = state;
    }
}

impl FrontendDataProvider for EmbeddedDataProvider {
    fn get_printer_status(&self) -> PrinterStatus {
        // Reserved implementation
        // TODO: Implement shared memory read
        let shared = self.shared_mem.blocking_read();
        PrinterStatus::new(format!("state_{}", shared.printer_state))
    }
    
    fn get_temperature(&self) -> TempStatus {
        // Reserved implementation
        let shared = self.shared_mem.blocking_read();
        TempStatus::new(
            shared.hotend_current,
            shared.hotend_target,
            shared.bed_current,
            shared.bed_target,
        )
    }
    
    fn get_position(&self) -> PositionData {
        // Reserved implementation
        let shared = self.shared_mem.blocking_read();
        PositionData::new(
            shared.position_x,
            shared.position_y,
            shared.position_z,
            shared.position_e,
        )
    }
    
    fn send_gcode(&self, cmd: &str) -> EmbResult<()> {
        // Reserved implementation
        // TODO: Implement shared memory command queue
        log::info!("EmbeddedDataProvider: Sending G-code (reserved): {}", cmd);
        Ok(())
    }
}

/// Web data provider (WebSocket)
/// For Web UI communication
pub struct WebDataProvider {
    /// WebSocket broadcast sender
    broadcast_tx: broadcast::Sender<crate::common::WebSocketMessage>,
    
    /// Cached printer status
    cached_status: Arc<RwLock<PrinterStatus>>,
    
    /// Cached temperature status
    cached_temp: Arc<RwLock<TempStatus>>,
    
    /// Cached position data
    cached_position: Arc<RwLock<PositionData>>,
}

impl WebDataProvider {
    /// Create a new Web data provider
    pub fn new(broadcast_tx: broadcast::Sender<crate::common::WebSocketMessage>) -> Self {
        Self {
            broadcast_tx,
            cached_status: Arc::new(RwLock::new(PrinterStatus::idle())),
            cached_temp: Arc::new(RwLock::new(TempStatus::new(0.0, 0.0, 0.0, 0.0))),
            cached_position: Arc::new(RwLock::new(PositionData::zero())),
        }
    }
    
    /// Broadcast message to WebSocket clients
    pub fn broadcast(&self, message: crate::common::WebSocketMessage) -> EmbResult<()> {
        self.broadcast_tx.send(message)
            .map_err(|e| crate::common::EmbError::Communication(format!("Broadcast failed: {}", e)))?;
        Ok(())
    }
    
    /// Update cached status and broadcast
    pub async fn update_and_broadcast_status(&self, status: PrinterStatus) -> EmbResult<()> {
        let mut cached = self.cached_status.write().await;
        *cached = status.clone();
        
        // Broadcast state change
        self.broadcast(crate::common::WebSocketMessage::State {
            from: cached.state.clone(),
            to: status.state,
        })?;
        
        Ok(())
    }
}

impl FrontendDataProvider for WebDataProvider {
    fn get_printer_status(&self) -> PrinterStatus {
        self.cached_status.blocking_read().clone()
    }
    
    fn get_temperature(&self) -> TempStatus {
        self.cached_temp.blocking_read().clone()
    }
    
    fn get_position(&self) -> PositionData {
        self.cached_position.blocking_read().clone()
    }
    
    fn send_gcode(&self, cmd: &str) -> EmbResult<()> {
        // TODO: Implement WebSocket command sending
        log::info!("WebDataProvider: Sending G-code: {}", cmd);
        Ok(())
    }
}

impl Default for EmbeddedDataProvider {
    fn default() -> Self {
        Self::new()
    }
}