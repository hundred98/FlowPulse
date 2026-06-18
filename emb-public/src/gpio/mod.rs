//! GPIO management module
//!
//! Provides GPIO pin control and standardized event subscription
//! for monitoring GPIO state changes from the device.

pub mod manager;
pub use manager::{GpioManager, GpioEvent};

