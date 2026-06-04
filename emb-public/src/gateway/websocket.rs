//! WebSocket server for Web UI
//!
//! Provides WebSocket-based communication channel for web-based user interfaces.

use crate::{EmbResult, EmbError};
use crate::message_queue::{Message, MessageType, MessageQueue};
use crate::state::DeviceStateManager;
use crate::common::WebSocketMessage;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use log::info;

/// WebSocket server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// Server bind address
    pub bind_address: String,
    /// Server port
    pub port: u16,
    /// Maximum connections
    pub max_connections: usize,
    /// Enable authentication
    pub enable_auth: bool,
    /// Authentication token (optional)
    pub auth_token: Option<String>,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            max_connections: 10,
            enable_auth: false,
            auth_token: None,
        }
    }
}

/// WebSocket server for Web UI
pub struct WebSocketServer {
    /// Server configuration
    config: WebSocketConfig,
    /// Message queue
    message_queue: Arc<MessageQueue>,
    /// Device state manager
    device_state: Arc<DeviceStateManager>,
    /// Active connections
    connections: Arc<RwLock<Vec<WebSocketConnection>>>,
    /// Server status
    status: Arc<RwLock<WebSocketStatus>>,
}

/// WebSocket connection information
#[derive(Debug, Clone)]
pub struct WebSocketConnection {
    /// Connection ID
    pub id: usize,
    /// Client address
    pub client_addr: String,
    /// Connection time
    pub connected_at: chrono::DateTime<chrono::Utc>,
    /// Is authenticated
    pub authenticated: bool,
}

/// WebSocket server status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketStatus {
    /// Is server running
    pub running: bool,
    /// Number of active connections
    pub active_connections: usize,
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
}

impl Default for WebSocketStatus {
    fn default() -> Self {
        Self {
            running: false,
            active_connections: 0,
            messages_sent: 0,
            messages_received: 0,
        }
    }
}

impl WebSocketServer {
    /// Create a new WebSocket server
    pub fn new(
        config: WebSocketConfig,
        message_queue: Arc<MessageQueue>,
        device_state: Arc<DeviceStateManager>,
    ) -> Self {
        Self {
            config,
            message_queue,
            device_state,
            connections: Arc::new(RwLock::new(Vec::new())),
            status: Arc::new(RwLock::new(WebSocketStatus::default())),
        }
    }
    
    /// Start the WebSocket server
    pub async fn start(&self) -> EmbResult<()> {
        let mut status = self.status.write().await;
        status.running = true;
        
        info!("WebSocket server started on {}:{}", self.config.bind_address, self.config.port);
        
        // TODO: Implement actual WebSocket server using tokio-tungstenite or axum
        // For now, we just mark the server as running
        
        Ok(())
    }
    
    /// Stop the WebSocket server
    pub async fn stop(&self) -> EmbResult<()> {
        let mut status = self.status.write().await;
        status.running = false;
        
        // Clear all connections
        let mut connections = self.connections.write().await;
        connections.clear();
        
        info!("WebSocket server stopped");
        Ok(())
    }
    
    /// Handle incoming WebSocket message
    pub async fn handle_message(&self, msg: WebSocketMessage) -> EmbResult<()> {
        // Convert WebSocket message to queue message
        let queue_msg = self.convert_websocket_message(msg)?;
        
        // Enqueue message
        self.message_queue.enqueue(queue_msg).await?;
        
        // Update statistics
        let mut status = self.status.write().await;
        status.messages_received += 1;
        
        Ok(())
    }
    
    /// Send message to WebSocket clients
    pub async fn send_to_clients(&self, _msg: WebSocketMessage) -> EmbResult<()> {
        // TODO: Implement actual message sending to WebSocket clients
        
        // Update statistics
        let mut status = self.status.write().await;
        status.messages_sent += 1;
        
        Ok(())
    }
    
    /// Broadcast printer status to all clients
    pub async fn broadcast_status(&self) -> EmbResult<()> {
        // Get current device state
        let position = self.device_state.get_position().await;
        let temperatures = self.device_state.get_temperatures().await;
        
        // Create temperature message
        let temp_msg = WebSocketMessage::Temperature {
            hotend_current: *temperatures.get("hotend").unwrap_or(&0.0),
            hotend_target: *temperatures.get("hotend_target").unwrap_or(&0.0),
            bed_current: *temperatures.get("bed").unwrap_or(&0.0),
            bed_target: *temperatures.get("bed_target").unwrap_or(&0.0),
        };
        
        // Create position message
        let pos_msg = WebSocketMessage::Position {
            x: position.x,
            y: position.y,
            z: position.z,
            e: position.e,
        };
        
        // Broadcast to all clients
        self.send_to_clients(temp_msg).await?;
        self.send_to_clients(pos_msg).await?;
        
        Ok(())
    }
    
    /// Convert WebSocket message to queue message
    fn convert_websocket_message(&self, msg: WebSocketMessage) -> EmbResult<Message> {
        match msg {
            WebSocketMessage::PrintEvent { event, message } => {
                let msg_type = match event.as_str() {
                    "start" => MessageType::PrintStart,
                    "pause" => MessageType::PrintPause,
                    "resume" => MessageType::PrintResume,
                    "stop" => MessageType::PrintStop,
                    _ => MessageType::Custom(event.clone()),
                };
                
                Ok(Message::new(
                    msg_type,
                    "websocket".to_string(),
                    serde_json::json!({
                        "message": message,
                    }),
                ))
            },
            WebSocketMessage::State { from, to } => {
                Ok(Message::new(
                    MessageType::StateQuery,
                    "websocket".to_string(),
                    serde_json::json!({
                        "from": from,
                        "to": to,
                    }),
                ))
            },
            WebSocketMessage::Alert { severity, message } => {
                Ok(Message::new(
                    MessageType::PrintError,
                    "websocket".to_string(),
                    serde_json::json!({
                        "severity": severity,
                        "message": message,
                    }),
                ))
            },
            _ => {
                Ok(Message::new(
                    MessageType::Custom("unknown".to_string()),
                    "websocket".to_string(),
                    serde_json::json!({}),
                ))
            }
        }
    }
    
    /// Get server status
    pub async fn get_status(&self) -> WebSocketStatus {
        self.status.read().await.clone()
    }
    
    /// Get active connections
    pub async fn get_connections(&self) -> Vec<WebSocketConnection> {
        self.connections.read().await.clone()
    }
    
    /// Add a new connection
    pub async fn add_connection(&self, conn: WebSocketConnection) -> EmbResult<()> {
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
    fn test_websocket_config_default() {
        let config = WebSocketConfig::default();
        assert_eq!(config.bind_address, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert_eq!(config.max_connections, 10);
    }
}