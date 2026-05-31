//! Communication Gateway Module
//!
//! Provides communication channel management.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelType {
    UnixSocket,
    Serial,
    DDS,
    MQTT,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChannelStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
    Reconnecting,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Direction {
    Upstream,
    Downstream,
    Bidirectional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub channel_type: ChannelType,
    pub address: String,
    pub direction: Direction,
    pub timeout_ms: u64,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            channel_type: ChannelType::Serial,
            address: "/dev/ttyUSB0".to_string(),
            direction: Direction::Bidirectional,
            timeout_ms: 5000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub frames_sent: u64,
    pub frames_received: u64,
    pub errors: u64,
    pub last_error: Option<String>,
}

impl Default for ChannelStats {
    fn default() -> Self {
        Self {
            bytes_sent: 0,
            bytes_received: 0,
            frames_sent: 0,
            frames_received: 0,
            errors: 0,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommunicationChannel {
    pub config: ChannelConfig,
    pub status: ChannelStatus,
    pub stats: ChannelStats,
}

impl CommunicationChannel {
    pub fn new(config: ChannelConfig) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            stats: ChannelStats::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SerialChannelStats {
    pub base: ChannelStats,
    pub frames_sent: u64,
    pub frames_received: u64,
    pub frames_valid: u64,
    pub frames_invalid: u64,
    pub crc_errors: u64,
    pub bytes_per_second: f64,
    pub connected_since: Option<u64>,
}

impl Default for SerialChannelStats {
    fn default() -> Self {
        Self {
            base: ChannelStats::default(),
            frames_sent: 0,
            frames_received: 0,
            frames_valid: 0,
            frames_invalid: 0,
            crc_errors: 0,
            bytes_per_second: 0.0,
            connected_since: None,
        }
    }
}

pub struct CommunicationGateway {
    channels: Arc<Mutex<HashMap<String, CommunicationChannel>>>,
}

impl CommunicationGateway {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add_channel(&self, name: String, config: ChannelConfig) {
        let mut channels = self.channels.lock().unwrap();
        channels.insert(name, CommunicationChannel::new(config));
    }

    pub fn get_channel(&self, name: &str) -> Option<CommunicationChannel> {
        let channels = self.channels.lock().unwrap();
        channels.get(name).cloned()
    }

    pub fn list_channels(&self) -> Vec<String> {
        let channels = self.channels.lock().unwrap();
        channels.keys().cloned().collect()
    }

    pub fn remove_channel(&self, name: &str) {
        let mut channels = self.channels.lock().unwrap();
        channels.remove(name);
    }
}

impl Default for CommunicationGateway {
    fn default() -> Self {
        Self::new()
    }
}
