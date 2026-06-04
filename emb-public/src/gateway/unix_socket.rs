//! UnixSocket server for CLI and HMI
//!
//! Provides Unix domain socket-based communication channel for local CLI and HMI interfaces.

use crate::{EmbResult, EmbError};
use crate::message_queue::{Message, MessageType, MessageQueue};
use crate::state::DeviceStateManager;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use log::{info, warn, error};
use std::path::PathBuf;

/// UnixSocket server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnixSocketConfig {
    /// Socket file path
    pub socket_path: String,
    /// Maximum connections
    pub max_connections: usize,
    /// Buffer size for reading
    pub buffer_size: usize,
    /// Enable HMI mode (shared memory fallback)
    pub enable_hmi_mode: bool,
}

impl Default for UnixSocketConfig {
    fn default() -> Self {
        Self {
            socket_path: "/tmp/flowpulse.sock".to_string(),
            max_connections: 5,
            buffer_size: 4096,
            enable_hmi_mode: false,
        }
    }
}

/// UnixSocket server for CLI and HMI
pub struct UnixSocketServer {
    /// Server configuration
    config: UnixSocketConfig,
    /// Message queue
    message_queue: Arc<MessageQueue>,
    /// Device state manager
    device_state: Arc<DeviceStateManager>,
    /// Active connections
    connections: Arc<RwLock<Vec<UnixSocketConnection>>>,
    /// Server status
    status: Arc<RwLock<UnixSocketStatus>>,
}

/// UnixSocket connection information
#[derive(Debug, Clone)]
pub struct UnixSocketConnection {
    /// Connection ID
    pub id: usize,
    /// Client type (CLI or HMI)
    pub client_type: UnixSocketClientType,
    /// Connection time
    pub connected_at: chrono::DateTime<chrono::Utc>,
    /// Is authenticated
    pub authenticated: bool,
}

/// UnixSocket client type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnixSocketClientType {
    CLI,
    HMI,
    Unknown,
}

/// UnixSocket server status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnixSocketStatus {
    /// Is server running
    pub running: bool,
    /// Number of active connections
    pub active_connections: usize,
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Socket file path
    pub socket_path: String,
}

impl Default for UnixSocketStatus {
    fn default() -> Self {
        Self {
            running: false,
            active_connections: 0,
            messages_sent: 0,
            messages_received: 0,
            socket_path: "/tmp/flowpulse.sock".to_string(),
        }
    }
}

impl UnixSocketServer {
    /// Create a new UnixSocket server
    pub fn new(
        config: UnixSocketConfig,
        message_queue: Arc<MessageQueue>,
        device_state: Arc<DeviceStateManager>,
    ) -> Self {
        Self {
            config,
            message_queue,
            device_state,
            connections: Arc::new(RwLock::new(Vec::new())),
            status: Arc::new(RwLock::new(UnixSocketStatus {
                socket_path: config.socket_path.clone(),
                ..Default::default()
            })),
        }
    }
    
    /// Start the UnixSocket server
    pub async fn start(&self) -> EmbResult<()> {
        // Check if socket file already exists
        let socket_path = PathBuf::from(&self.config.socket_path);
        if socket_path.exists() {
            // Remove existing socket file
            std::fs::remove_file(&socket_path)?;
            info!("Removed existing socket file: {}", self.config.socket_path);
        }
        
        let mut status = self.status.write().await;
        status.running = true;
        
        info!("UnixSocket server started at {}", self.config.socket_path);
        
        // TODO: Implement actual UnixSocket server using tokio-net
        // For now, we just mark the server as running
        
        Ok(())
    }
    
    /// Stop the UnixSocket server
    pub async fn stop(&self) -> EmbResult<()> {
        let mut status = self.status.write().await;
        status.running = false;
        
        // Clear all connections
        let mut connections = self.connections.write().await;
        connections.clear();
        
        // Remove socket file
        let socket_path = PathBuf::from(&self.config.socket_path);
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
            info!("Removed socket file: {}", self.config.socket_path);
        }
        
        info!("UnixSocket server stopped");
        Ok(())
    }
    
    /// Handle incoming UnixSocket message
    pub async fn handle_message(&self, raw_data: &[u8], client_type: UnixSocketClientType) -> EmbResult<()> {
        // Parse raw data as JSON
        let msg_data: serde_json::Value = serde_json::from_slice(raw_data)
            .map_err(|e| EmbError::Gateway(format!("Failed to parse UnixSocket message: {}", e)))?;
        
        // Convert to queue message
        let queue_msg = self.convert_unixsocket_message(msg_data, client_type)?;
        
        // Enqueue message
        self.message_queue.enqueue(queue_msg).await?;
        
        // Update statistics
        let mut status = self.status.write().await;
        status.messages_received += 1;
        
        Ok(())
    }
    
    /// Send message to UnixSocket client
    pub async fn send_to_client(&self, conn_id: usize, msg: serde_json::Value) -> EmbResult<()> {
        // TODO: Implement actual message sending to UnixSocket client
        
        // Update statistics
        let mut status = self.status.write().await;
        status.messages_sent += 1;
        
        Ok(())
    }
    
    /// Broadcast message to all UnixSocket clients
    pub async fn broadcast(&self, msg: serde_json::Value) -> EmbResult<()> {
        let connections = self.connections.read().await;
        
        for conn in connections.iter() {
            self.send_to_client(conn.id, msg.clone()).await?;
        }
        
        Ok(())
    }
    
    /// Convert UnixSocket message to queue message
    fn convert_unixsocket_message(&self, msg_data: serde_json::Value, client_type: UnixSocketClientType) -> EmbResult<Message> {
        let command = msg_data.get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        let params = msg_data.get("params")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        
        let source = match client_type {
            UnixSocketClientType::CLI => "unix_socket_cli",
            UnixSocketClientType::HMI => "unix_socket_hmi",
            UnixSocketClientType::Unknown => "unix_socket",
        };
        
        let msg_type = match command {
            "start" => MessageType::PrintStart,
            "pause" => MessageType::PrintPause,
            "resume" => MessageType::PrintResume,
            "stop" => MessageType::PrintStop,
            "home" => MessageType::HomeCommand,
            "move" => MessageType::MoveCommand,
            "temp" => MessageType::TemperatureSet,
            "status" => MessageType::StateQuery,
            "temp_get" => MessageType::TemperatureGet,
            "hw_status" => MessageType::HardwareStatus,
            _ => MessageType::Custom(command.to_string()),
        };
        
        Ok(Message::new(
            msg_type,
            source.to_string(),
            params,
        ))
    }
    
    /// Get server status
    pub async fn get_status(&self) -> UnixSocketStatus {
        self.status.read().await.clone()
    }
    
    /// Get active connections
    pub async fn get_connections(&self) -> Vec<UnixSocketConnection> {
        self.connections.read().await.clone()
    }
    
    /// Add a new connection
    pub async fn add_connection(&self, conn: UnixSocketConnection) -> EmbResult<()> {
        let mut connections = self.connections.write().await;
        
        // Check max connections limit
        if connections.len() >= self.config.max_connections {
            return Err(EmbError::Gateway("Maximum connections reached".to_string()));
        }
        
        connections.push(conn);
        
        // Update status
        let mut status = self.status.write().await;
        status.active_connections = connections.len();
        
        Ok(())
    }
    
    /// Remove a connection
    pub async fn remove_connection(&self, conn_id: usize) -> EmbResult<()> {
        let mut connections = self.connections.write().await;
        connections.retain(|c| c.id != conn_id);
        
        // Update status
        let mut status = self.status.write().await;
        status.active_connections = connections.len();
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_unixsocket_config_default() {
        let config = UnixSocketConfig::default();
        assert_eq!(config.socket_path, "/tmp/flowpulse.sock");
        assert_eq!(config.max_connections, 5);
        assert_eq!(config.buffer_size, 4096);
    }
}