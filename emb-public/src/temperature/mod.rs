//! Temperature control module
//!
//! Manages heater zones and temperature monitoring.

pub mod controller;
pub mod zone;

pub use controller::TemperatureController;
pub use zone::{
    HeaterConfig, HeaterZone, PidParams, TemperatureState, TemperatureStatus,
};