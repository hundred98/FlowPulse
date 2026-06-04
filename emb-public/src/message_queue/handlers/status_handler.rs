//! Status handler for querying printer status

use crate::{EmbResult, EmbError};
use crate::state::DeviceStateManager;
use crate::state_machine::StateMachine;
use crate::print_control::PrintController;
use crate::message_queue::{MessageHandler, Message, MessageType};
use async_trait::async_trait;
use std::sync::Arc;
use serde_json::json;

/// Status handler for querying printer status
pub struct StatusHandler {
    /// Device state manager
    device_state: Arc<DeviceStateManager>,
    
    /// State machine
    state_machine: Arc<StateMachine>,
    
    /// Print controller
    print_controller: Arc<PrintController>,
}

impl StatusHandler {
    /// Create a new status handler
    pub fn new(
        device_state: Arc<DeviceStateManager>,
        state_machine: Arc<StateMachine>,
        print_controller: Arc<PrintController>,
    ) -> Self {
        Self {
            device_state,
            state_machine,
            print_controller,
        }
    }
    
    /// Handle state query
    async fn handle_state_query(&self, message: &mut Message) -> EmbResult<()> {
        // Get current printer state
        let state = self.state_machine.get_state();
        
        // Get device state
        let position = self.device_state.get_position().await;
        let motion_status = self.device_state.get_motion_status().await;
        let temperatures = self.device_state.get_temperatures().await;
        
        // Build response payload
        message.payload = json!({
            "state": format!("{:?}", state),
            "position": {
                "x": position.x,
                "y": position.y,
                "z": position.z,
                "e": position.e,
            },
            "motion_status": format!("{:?}", motion_status),
            "temperatures": temperatures,
        });
        
        log::debug!("State query: {:?}", state);
        Ok(())
    }
    
    /// Handle temperature get
    async fn handle_temperature_get(&self, message: &mut Message) -> EmbResult<()> {
        // Get temperatures
        let temperatures = self.device_state.get_temperatures().await;
        
        // Check if specific heater is requested
        let heater = message.payload.get("heater").and_then(|v| v.as_str());
        
        if let Some(heater_name) = heater {
            // Return specific heater temperature
            let temp = temperatures.get(heater_name).copied().unwrap_or(0.0);
            message.payload = json!({
                "heater": heater_name,
                "temperature": temp,
            });
        } else {
            // Return all temperatures
            message.payload = json!({
                "temperatures": temperatures,
            });
        }
        
        log::debug!("Temperature get: {:?}", temperatures);
        Ok(())
    }
    
    /// Handle hardware status query
    async fn handle_hardware_status(&self, message: &mut Message) -> EmbResult<()> {
        // Get device state
        let position = self.device_state.get_position().await;
        let motion_status = self.device_state.get_motion_status().await;
        let flow_status = self.device_state.get_flow_status().await;
        let temperatures = self.device_state.get_temperatures().await;
        
        // Build hardware status response
        message.payload = json!({
            "position": {
                "x": position.x,
                "y": position.y,
                "z": position.z,
                "e": position.e,
            },
            "motion_status": format!("{:?}", motion_status),
            "flow_status": {
                "flow_rate": flow_status.flow_rate,
                "pressure": flow_status.pressure,
                "is_active": flow_status.is_active,
            },
            "temperatures": temperatures,
            "is_stale": self.device_state.is_stale(5000).await,
        });
        
        log::debug!("Hardware status query");
        Ok(())
    }
    
    /// Handle print progress query
    async fn handle_print_progress(&self, message: &mut Message) -> EmbResult<()> {
        // Get print progress
        let progress = self.print_controller.get_progress().await;
        
        // Build progress response
        message.payload = json!({
            "percent": progress.percent,
            "current_layer": progress.current_layer,
            "total_layers": progress.total_layers,
            "elapsed_seconds": progress.elapsed_seconds,
            "remaining_seconds": progress.remaining_seconds,
        });
        
        log::debug!("Print progress: {}%", progress.percent);
        Ok(())
    }
}

#[async_trait]
impl MessageHandler for StatusHandler {
    async fn handle(&self, message: &mut Message) -> EmbResult<()> {
        match message.message_type {
            MessageType::StateQuery => self.handle_state_query(message).await,
            MessageType::TemperatureGet => self.handle_temperature_get(message).await,
            MessageType::HardwareStatus => self.handle_hardware_status(message).await,
            MessageType::PrintStart | MessageType::PrintPause | MessageType::PrintResume | MessageType::PrintStop => {
                self.handle_print_progress(message).await
            },
            _ => Err(EmbError::MessageQueue(format!("Unsupported message type: {:?}", message.message_type))),
        }
    }
    
    fn name(&self) -> &str {
        "StatusHandler"
    }
}