//! Channel Manager for unified access
//!
//! Manages all communication channels (WebSocket, UnixSocket, MQTT) and provides
//! a unified interface for message routing and state synchronization.

use crate::{EmbResult};
use crate::message_queue::{Message, MessageQueue};
use crate::state::DeviceStateManager;
use crate::common::{WebSocketMessage, SyncEventPublisher, PrinterEvent};
use super::{WebSocketServer, WebSocketConfig, UnixSocketServer, UnixSocketConfig, MqttClient, MqttConfig};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use log::{info, warn};

/// Channel manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelManagerConfig {
    /// WebSocket configuration
    pub websocket: WebSocketConfig,
    /// UnixSocket configuration
    pub unix_socket: UnixSocketConfig,
    /// MQTT configuration
    pub mqtt: MqttConfig,
    /// Enable WebSocket channel
    pub enable_websocket: bool,
    /// Enable UnixSocket channel
    pub enable_unix_socket: bool,
    /// Enable MQTT channel
    pub enable_mqtt: bool,
    /// Status broadcast interval (seconds)
    pub status_broadcast_interval: u64,
}

impl Default for ChannelManagerConfig {
    fn default() -> Self {
        Self {
            websocket: WebSocketConfig::default(),
            unix_socket: UnixSocketConfig::default(),
            mqtt: MqttConfig::default(),
            enable_websocket: true,
            enable_unix_socket: true,
            enable_mqtt: false,
            status_broadcast_interval: 1,
        }
    }
}

/// Channel manager for unified access
pub struct ChannelManager {
    /// Manager configuration
    config: ChannelManagerConfig,
    /// WebSocket server
    websocket_server: Option<Arc<WebSocketServer>>,
    /// UnixSocket server
    unix_socket_server: Option<Arc<UnixSocketServer>>,
    /// MQTT client
    mqtt_client: Option<Arc<MqttClient>>,
    /// Message queue
    message_queue: Arc<MessageQueue>,
    /// Device state manager
    device_state: Arc<DeviceStateManager>,
    /// Event publisher
    event_publisher: Arc<SyncEventPublisher>,
    /// Manager status
    status: Arc<RwLock<ChannelManagerStatus>>,
}

/// Channel manager status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelManagerStatus {
    /// WebSocket server status
    pub websocket_running: bool,
    /// UnixSocket server status
    pub unix_socket_running: bool,
    /// MQTT client status
    pub mqtt_connected: bool,
    /// Total messages routed
    pub total_messages_routed: u64,
    /// Last status broadcast time
    pub last_broadcast: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for ChannelManagerStatus {
    fn default() -> Self {
        Self {
            websocket_running: false,
            unix_socket_running: false,
            mqtt_connected: false,
            total_messages_routed: 0,
            last_broadcast: None,
        }
    }
}

impl ChannelManager {
    /// Create a new channel manager
    pub fn new(
        config: ChannelManagerConfig,
        message_queue: Arc<MessageQueue>,
        device_state: Arc<DeviceStateManager>,
        event_publisher: Arc<SyncEventPublisher>,
    ) -> Self {
        // Create WebSocket server if enabled
        let websocket_server = if config.enable_websocket {
            Some(Arc::new(WebSocketServer::new(
                config.websocket.clone(),
                message_queue.clone(),
                device_state.clone(),
            )))
        } else {
            None
        };
        
        // Create UnixSocket server if enabled
        let unix_socket_server = if config.enable_unix_socket {
            Some(Arc::new(UnixSocketServer::new(
                config.unix_socket.clone(),
                message_queue.clone(),
                device_state.clone(),
            )))
        } else {
            None
        };
        
        // Create MQTT client if enabled
        let mqtt_client = if config.enable_mqtt {
            Some(Arc::new(MqttClient::new(
                config.mqtt.clone(),
                message_queue.clone(),
                device_state.clone(),
            )))
        } else {
            None
        };
        
        Self {
            config,
            websocket_server,
            unix_socket_server,
            mqtt_client,
            message_queue,
            device_state,
            event_publisher,
            status: Arc::new(RwLock::new(ChannelManagerStatus::default())),
        }
    }
    
    /// Start all enabled channels
    pub async fn start_all(&self) -> EmbResult<()> {
        // Start WebSocket server
        if let Some(ref ws_server) = self.websocket_server {
            ws_server.start().await?;
            self.status.write().await.websocket_running = true;
            info!("WebSocket server started");
        }
        
        // Start UnixSocket server
        if let Some(ref unix_server) = self.unix_socket_server {
            unix_server.start().await?;
            self.status.write().await.unix_socket_running = true;
            info!("UnixSocket server started");
        }
        
        // Connect MQTT client
        if let Some(ref mqtt_client) = self.mqtt_client {
            mqtt_client.connect().await?;
            self.status.write().await.mqtt_connected = true;
            info!("MQTT client connected");
        }
        
        // Start status broadcast loop
        self.start_status_broadcast().await?;
        
        info!("All channels started");
        Ok(())
    }
    
    /// Stop all channels
    pub async fn stop_all(&self) -> EmbResult<()> {
        // Stop WebSocket server
        if let Some(ref ws_server) = self.websocket_server {
            ws_server.stop().await?;
            self.status.write().await.websocket_running = false;
            info!("WebSocket server stopped");
        }
        
        // Stop UnixSocket server
        if let Some(ref unix_server) = self.unix_socket_server {
            unix_server.stop().await?;
            self.status.write().await.unix_socket_running = false;
            info!("UnixSocket server stopped");
        }
        
        // Disconnect MQTT client
        if let Some(ref mqtt_client) = self.mqtt_client {
            mqtt_client.disconnect().await?;
            self.status.write().await.mqtt_connected = false;
            info!("MQTT client disconnected");
        }
        
        info!("All channels stopped");
        Ok(())
    }
    
    /// Route message to appropriate channel
    pub async fn route_message(&self, msg: Message) -> EmbResult<()> {
        // Determine destination based on message source and type
        let destination = msg.destination.clone();
        
        // Route to specific channel if destination is specified
        if let Some(dest) = destination {
            match dest.as_str() {
                "websocket" => {
                    if let Some(ref ws_server) = self.websocket_server {
                        // Convert to WebSocket message and send
                        let ws_msg = self.convert_to_websocket_message(msg)?;
                        ws_server.send_to_clients(ws_msg).await?;
                    }
                },
                "unix_socket" => {
                    if let Some(ref unix_server) = self.unix_socket_server {
                        // Broadcast to UnixSocket clients
                        unix_server.broadcast(msg.payload.clone()).await?;
                    }
                },
                "mqtt" => {
                    if let Some(ref mqtt_client) = self.mqtt_client {
                        // Publish to MQTT
                        mqtt_client.publish(super::mqtt::MqttTopicType::Custom(dest), msg.payload.clone()).await?;
                    }
                },
                _ => {
                    warn!("Unknown destination: {}", dest);
                }
            }
        } else {
            // Broadcast to all active channels
            self.broadcast_message(msg).await?;
        }
        
        // Update statistics
        self.status.write().await.total_messages_routed += 1;
        
        Ok(())
    }
    
    /// Broadcast message to all active channels
    pub async fn broadcast_message(&self, msg: Message) -> EmbResult<()> {
        // Broadcast to WebSocket clients
        if let Some(ref ws_server) = self.websocket_server {
            let ws_msg = self.convert_to_websocket_message(msg.clone())?;
            ws_server.send_to_clients(ws_msg).await?;
        }
        
        // Broadcast to UnixSocket clients
        if let Some(ref unix_server) = self.unix_socket_server {
            unix_server.broadcast(msg.payload.clone()).await?;
        }
        
        // Publish to MQTT
        if let Some(ref mqtt_client) = self.mqtt_client {
            mqtt_client.publish(super::mqtt::MqttTopicType::Event, msg.payload.clone()).await?;
        }
        
        Ok(())
    }
    
    /// Start status broadcast loop
    async fn start_status_broadcast(&self) -> EmbResult<()> {
        // TODO: Implement periodic status broadcast
        
        info!("Status broadcast started with interval {}s", self.config.status_broadcast_interval);
        Ok(())
    }
    
    /// Broadcast status to all channels
    pub async fn broadcast_status(&self) -> EmbResult<()> {
        // Broadcast to WebSocket
        if let Some(ref ws_server) = self.websocket_server {
            ws_server.broadcast_status().await?;
        }
        
        // Publish to MQTT
        if let Some(ref mqtt_client) = self.mqtt_client {
            mqtt_client.publish_status().await?;
        }
        
        // Update last broadcast time
        self.status.write().await.last_broadcast = Some(chrono::Utc::now());
        
        Ok(())
    }
    
    /// Convert queue message to WebSocket message
    fn convert_to_websocket_message(&self, _msg: Message) -> EmbResult<WebSocketMessage> {
        // TODO: Implement proper conversion based on message type
        
        Ok(WebSocketMessage::State {
            from: "unknown".to_string(),
            to: "unknown".to_string(),
        })
    }
    
    /// Get manager status
    pub async fn get_status(&self) -> ChannelManagerStatus {
        self.status.read().await.clone()
    }
    
    /// Get WebSocket server
    pub fn get_websocket_server(&self) -> Option<Arc<WebSocketServer>> {
        self.websocket_server.clone()
    }
    
    /// Get UnixSocket server
    pub fn get_unix_socket_server(&self) -> Option<Arc<UnixSocketServer>> {
        self.unix_socket_server.clone()
    }
    
    /// Get MQTT client
    pub fn get_mqtt_client(&self) -> Option<Arc<MqttClient>> {
        self.mqtt_client.clone()
    }
    
    /// Handle event from event system
    pub async fn handle_event(&self, event: PrinterEvent) -> EmbResult<()> {
        // Convert event to message
        let msg = Message::new(
            crate::message_queue::MessageType::SystemEvent,
            "event_system".to_string(),
            serde_json::json!({
                "kind": event.kind.to_string(),
                "source": event.source,
                "message": event.message,
                "severity": event.severity.to_string(),
                "timestamp": event.timestamp.to_rfc3339(),
            }),
        );
        
        // Broadcast event to all channels
        self.broadcast_message(msg).await?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_channel_manager_config_default() {
        let config = ChannelManagerConfig::default();
        assert!(config.enable_websocket);
        assert!(config.enable_unix_socket);
        assert!(!config.enable_mqtt);
        assert_eq!(config.status_broadcast_interval, 1);
    }
}