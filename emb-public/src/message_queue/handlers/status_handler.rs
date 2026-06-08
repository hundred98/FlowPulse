//! Status handler for querying printer status

use crate::{EmbResult, EmbError};
use crate::state::DeviceStateManager;
use crate::state_machine::StateMachine;
use crate::print_control::PrintController;
use crate::temperature::TemperatureManager;
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

    /// Temperature manager
    temperature_manager: Arc<TemperatureManager>,
}

impl StatusHandler {
    /// Create a new status handler
    pub fn new(
        device_state: Arc<DeviceStateManager>,
        state_machine: Arc<StateMachine>,
        print_controller: Arc<PrintController>,
        temperature_manager: Arc<TemperatureManager>,
    ) -> Self {
        Self {
            device_state,
            state_machine,
            print_controller,
            temperature_manager,
        }
    }
    
    /// Handle state query
    async fn handle_state_query(&self, message: &mut Message) -> EmbResult<()> {
        // Get current printer state
        let state = self.state_machine.get_state();

        // Get device state
        let position = self.device_state.get_position().await;
        let motion_status = self.device_state.get_motion_status().await;
        let heaters = self.temperature_manager.get_all_heaters().await;

        // Convert heaters to simple temperature map
        let temperatures: std::collections::HashMap<String, f32> = heaters
            .iter()
            .map(|(name, state)| (name.clone(), state.current_temp))
            .collect();

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
        // Get heaters
        let heaters = self.temperature_manager.get_all_heaters().await;

        // Check if specific heater is requested
        let heater = message.payload.get("heater").and_then(|v| v.as_str());

        if let Some(heater_name) = heater {
            // Return specific heater temperature
            if let Some(heater_state) = heaters.get(heater_name) {
                message.payload = json!({
                    "heater": heater_name,
                    "current_temp": heater_state.current_temp,
                    "target_temp": heater_state.target_temp,
                    "is_heating": heater_state.is_heating,
                });
            } else {
                return Err(EmbError::InvalidParam(format!("Unknown heater: {}", heater_name)));
            }
        } else {
            // Return all heaters
            let temp_map: std::collections::HashMap<String, serde_json::Value> = heaters
                .iter()
                .map(|(name, state)| {
                    (
                        name.clone(),
                        json!({
                            "current_temp": state.current_temp,
                            "target_temp": state.target_temp,
                            "is_heating": state.is_heating,
                        }),
                    )
                })
                .collect();

            message.payload = json!({
                "heaters": temp_map,
            });
        }

        log::debug!("Temperature get: {} heaters", heaters.len());
        Ok(())
    }
    
    /// Handle hardware status query
    async fn handle_hardware_status(&self, message: &mut Message) -> EmbResult<()> {
        // Get device state
        let position = self.device_state.get_position().await;
        let motion_status = self.device_state.get_motion_status().await;
        let flow_status = self.device_state.get_flow_status().await;
        let heaters = self.temperature_manager.get_all_heaters().await;

        // Convert heaters to simple temperature map
        let temperatures: std::collections::HashMap<String, f32> = heaters
            .iter()
            .map(|(name, state)| (name.clone(), state.current_temp))
            .collect();

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
        Ok(()
)
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