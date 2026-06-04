//! MQTT client for remote monitoring
//!
//! Provides MQTT-based communication channel for remote monitoring and control.

use crate::{EmbResult, EmbError};
use crate::message_queue::{Message, MessageType, MessageQueue};
use crate::state::DeviceStateManager;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use log::info;

/// MQTT client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    /// MQTT broker address
    pub broker_address: String,
    /// MQTT broker port
    pub port: u16,
    /// Client ID
    pub client_id: String,
    /// Username (optional)
    pub username: Option<String>,
    /// Password (optional)
    pub password: Option<String>,
    /// Topic prefix
    pub topic_prefix: String,
    /// Enable TLS
    pub enable_tls: bool,
    /// Keep alive interval (seconds)
    pub keep_alive: u16,
    /// Clean session
    pub clean_session: bool,
    /// QoS level
    pub qos: u8,
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            broker_address: "mqtt.example.com".to_string(),
            port: 1883,
            client_id: "flowpulse_client".to_string(),
            username: None,
            password: None,
            topic_prefix: "flowpulse".to_string(),
            enable_tls: false,
            keep_alive: 60,
            clean_session: true,
            qos: 1,
        }
    }
}

/// MQTT client for remote monitoring
pub struct MqttClient {
    /// Client configuration
    config: MqttConfig,
    /// Message queue
    message_queue: Arc<MessageQueue>,
    /// Device state manager
    device_state: Arc<DeviceStateManager>,
    /// Client status
    status: Arc<RwLock<MqttStatus>>,
    /// Subscribed topics
    subscribed_topics: Arc<RwLock<Vec<String>>>,
}

/// MQTT client status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttStatus {
    /// Is client connected
    pub connected: bool,
    /// Broker address
    pub broker_address: String,
    /// Total messages published
    pub messages_published: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Last connection time
    pub last_connected: Option<chrono::DateTime<chrono::Utc>>,
    /// Last error
    pub last_error: Option<String>,
}

impl Default for MqttStatus {
    fn default() -> Self {
        Self {
            connected: false,
            broker_address: "".to_string(),
            messages_published: 0,
            messages_received: 0,
            last_connected: None,
            last_error: None,
        }
    }
}

/// MQTT topic types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MqttTopicType {
    Status,
    Command,
    Temperature,
    Position,
    Event,
    Custom(String),
}

impl MqttClient {
    /// Create a new MQTT client
    pub fn new(
        config: MqttConfig,
        message_queue: Arc<MessageQueue>,
        device_state: Arc<DeviceStateManager>,
    ) -> Self {
        let broker_address = format!("{}:{}", config.broker_address, config.port);
        Self {
            config,
            message_queue,
            device_state,
            status: Arc::new(RwLock::new(MqttStatus {
                broker_address,
                ..Default::default()
            })),
            subscribed_topics: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Connect to MQTT broker
    pub async fn connect(&self) -> EmbResult<()> {
        let mut status = self.status.write().await;
        status.connected = true;
        status.last_connected = Some(chrono::Utc::now());
        
        info!("MQTT client connected to {}:{}", self.config.broker_address, self.config.port);
        
        // TODO: Implement actual MQTT connection using rumqttc or mqtt-async-client
        // For now, we just mark the client as connected
        
        Ok(())
    }
    
    /// Disconnect from MQTT broker
    pub async fn disconnect(&self) -> EmbResult<()> {
        let mut status = self.status.write().await;
        status.connected = false;
        
        // Clear subscribed topics
        let mut topics = self.subscribed_topics.write().await;
        topics.clear();
        
        info!("MQTT client disconnected");
        Ok(())
    }
    
    /// Subscribe to a topic
    pub async fn subscribe(&self, topic_type: MqttTopicType) -> EmbResult<()> {
        let topic = self.build_topic(topic_type);
        
        // Add to subscribed topics list
        let mut topics = self.subscribed_topics.write().await;
        if !topics.contains(&topic) {
            topics.push(topic.clone());
        }
        
        info!("MQTT subscribed to topic: {}", topic);
        
        // TODO: Implement actual MQTT subscription
        
        Ok(())
    }
    
    /// Unsubscribe from a topic
    pub async fn unsubscribe(&self, topic_type: MqttTopicType) -> EmbResult<()> {
        let topic = self.build_topic(topic_type);
        
        // Remove from subscribed topics list
        let mut topics = self.subscribed_topics.write().await;
        topics.retain(|t| t != &topic);
        
        info!("MQTT unsubscribed from topic: {}", topic);
        
        // TODO: Implement actual MQTT unsubscription
        
        Ok(())
    }
    
    /// Publish message to a topic
    pub async fn publish(&self, topic_type: MqttTopicType, _payload: serde_json::Value) -> EmbResult<()> {
        if !self.status.read().await.connected {
            return Err(EmbError::Gateway("MQTT client not connected".to_string()));
        }
        
        let topic = self.build_topic(topic_type);
        
        // TODO: Implement actual MQTT publish
        
        // Update statistics
        let mut status = self.status.write().await;
        status.messages_published += 1;
        
        info!("MQTT published to topic: {}", topic);
        Ok(())
    }
    
    /// Handle incoming MQTT message
    pub async fn handle_message(&self, topic: &str, payload: &[u8]) -> EmbResult<()> {
        // Parse payload as JSON
        let msg_data: serde_json::Value = serde_json::from_slice(payload)
            .map_err(|e| EmbError::Gateway(format!("Failed to parse MQTT message: {}", e)))?;
        
        // Convert to queue message
        let queue_msg = self.convert_mqtt_message(topic, msg_data)?;
        
        // Enqueue message
        self.message_queue.enqueue(queue_msg).await?;
        
        // Update statistics
        let mut status = self.status.write().await;
        status.messages_received += 1;
        
        Ok(())
    }
    
    /// Publish printer status
    pub async fn publish_status(&self) -> EmbResult<()> {
        // Get current device state
        let position = self.device_state.get_position().await;
        let temperatures = self.device_state.get_temperatures().await;
        
        // Create status payload
        let status_payload = serde_json::json!({
            "position": {
                "x": position.x,
                "y": position.y,
                "z": position.z,
                "e": position.e,
            },
            "temperatures": temperatures,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        
        // Publish status
        self.publish(MqttTopicType::Status, status_payload).await?;
        
        Ok(())
    }
    
    /// Publish temperature update
    pub async fn publish_temperature(&self) -> EmbResult<()> {
        let temperatures = self.device_state.get_temperatures().await;
        
        let temp_payload = serde_json::json!({
            "temperatures": temperatures,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        
        self.publish(MqttTopicType::Temperature, temp_payload).await?;
        
        Ok(())
    }
    
    /// Publish position update
    pub async fn publish_position(&self) -> EmbResult<()> {
        let position = self.device_state.get_position().await;
        
        let pos_payload = serde_json::json!({
            "position": {
                "x": position.x,
                "y": position.y,
                "z": position.z,
                "e": position.e,
            },
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        
        self.publish(MqttTopicType::Position, pos_payload).await?;
        
        Ok(())
    }
    
    /// Build MQTT topic string
    fn build_topic(&self, topic_type: MqttTopicType) -> String {
        let suffix = match topic_type {
            MqttTopicType::Status => "status".to_string(),
            MqttTopicType::Command => "command".to_string(),
            MqttTopicType::Temperature => "temperature".to_string(),
            MqttTopicType::Position => "position".to_string(),
            MqttTopicType::Event => "event".to_string(),
            MqttTopicType::Custom(s) => s,
        };
        
        format!("{}/{}", self.config.topic_prefix, suffix)
    }
    
    /// Convert MQTT message to queue message
    fn convert_mqtt_message(&self, topic: &str, msg_data: serde_json::Value) -> EmbResult<Message> {
        // Determine message type based on topic
        let msg_type = if topic.contains("command") {
            let command = msg_data.get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            
            match command {
                "start" => MessageType::PrintStart,
                "pause" => MessageType::PrintPause,
                "resume" => MessageType::PrintResume,
                "stop" => MessageType::PrintStop,
                "home" => MessageType::HomeCommand,
                "move" => MessageType::MoveCommand,
                "temp" => MessageType::TemperatureSet,
                _ => MessageType::Custom(command.to_string()),
            }
        } else if topic.contains("status") {
            MessageType::StateQuery
        } else {
            MessageType::Custom(topic.to_string())
        };
        
        Ok(Message::new(
            msg_type,
            "mqtt".to_string(),
            msg_data,
        ))
    }
    
    /// Get client status
    pub async fn get_status(&self) -> MqttStatus {
        self.status.read().await.clone()
    }
    
    /// Get subscribed topics
    pub async fn get_subscribed_topics(&self) -> Vec<String> {
        self.subscribed_topics.read().await.clone()
    }
    
    /// Start periodic status publishing
    pub async fn start_status_publisher(&self, interval_secs: u64) -> EmbResult<()> {
        // TODO: Implement periodic status publishing
        
        info!("MQTT status publisher started with interval {}s", interval_secs);
        Ok(())
    }
    
    /// Stop periodic status publishing
    pub async fn stop_status_publisher(&self) -> EmbResult<()> {
        // TODO: Stop periodic status publishing
        
        info!("MQTT status publisher stopped");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mqtt_config_default() {
        let config = MqttConfig::default();
        assert_eq!(config.broker_address, "mqtt.example.com");
        assert_eq!(config.port, 1883);
        assert_eq!(config.client_id, "flowpulse_client");
        assert_eq!(config.topic_prefix, "flowpulse");
    }
    
    #[test]
    fn test_build_topic() {
        let config = MqttConfig::default();
        let client = MqttClient::new(
            config,
            Arc::new(MessageQueue::new(Default::default())),
            Arc::new(DeviceStateManager::new(
                Arc::new(crate::CoreSocketClient::new(Default::default())),
                Arc::new(crate::SyncEventPublisher::new()),
                Default::default(),
            )),
        );
        
        let topic = client.build_topic(MqttTopicType::Status);
        assert_eq!(topic, "flowpulse/status");
        
        let topic = client.build_topic(MqttTopicType::Command);
        assert_eq!(topic, "flowpulse/command");
    }
}