//! Communication Gateway Module
//!
//! Unified communication management between systems.
//!
//! Note: WebSocket functionality has been moved to `web-server` module.
//! Use `web-server::WebServer` for WebSocket support.

pub mod communication;
pub mod unix_socket;
pub mod mqtt;
pub mod channel_manager;

pub use communication::{
    CommunicationGateway, CommunicationChannel,
    ChannelType, ChannelStatus, ChannelConfig, ChannelStats, Direction,
    SerialChannelStats,
};

pub use unix_socket::{
    UnixSocketServer, UnixSocketConfig, UnixSocketConnection, UnixSocketStatus, UnixSocketClientType,
};

pub use mqtt::{
    MqttClient, MqttConfig, MqttStatus, MqttTopicType,
};

pub use channel_manager::{
    ChannelManager, ChannelManagerConfig, ChannelManagerStatus,
};
