//! State management module
//!
//! This module provides state management functionality for the printer system,
//! including device state synchronization and frontend data providers.

pub mod device_state;
pub mod frontend_provider;

pub use device_state::{
    DeviceStateManager, DeviceStateConfig, Position, MotionStatus, FlowStatus,
    DeviceStateSnapshot,
};
pub use frontend_provider::{
    FrontendDataProvider, UnixSocketProvider, EmbeddedDataProvider, WebDataProvider,
};