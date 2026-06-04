//! Communication Gateway Module
//!
//! Unified communication management between systems.

pub mod communication;
pub mod websocket;
pub mod unix_socket;
pub mod mqtt;
pub mod channel_manager;

pub use communication::{
    CommunicationGateway, CommunicationChannel,
    ChannelType, ChannelStatus, ChannelConfig, ChannelStats, Direction,
    SerialChannelStats,
};

pub use websocket::{
    WebSocketServer, WebSocketConfig, WebSocketConnection, WebSocketStatus,
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
