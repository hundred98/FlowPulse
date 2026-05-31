//! Communication Gateway Module
//!
//! Unified communication management between systems.

pub mod communication;

pub use communication::{
    CommunicationGateway, CommunicationChannel,
    ChannelType, ChannelStatus, ChannelConfig, ChannelStats, Direction,
    SerialChannelStats,
};
