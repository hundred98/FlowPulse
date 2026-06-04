//! Command handler for processing printer commands

use crate::{EmbResult, EmbError};
use crate::state::{DeviceStateManager, Position};
use crate::state_machine::{StateMachine, PrinterState, TransitionReason};
use crate::print_control::PrintController;
use crate::message_queue::{MessageHandler, Message, MessageType};
use async_trait::async_trait;
use std::sync::Arc;

/// Command handler for processing printer commands
pub struct CommandHandler {
    /// Device state manager
    device_state: Arc<DeviceStateManager>,
    
    /// State machine
    state_machine: Arc<StateMachine>,
    
    /// Print controller
    print_controller: Arc<PrintController>,
}

impl CommandHandler {
    /// Create a new command handler
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
    
    /// Handle print start command
    async fn handle_print_start(&self, message: &mut Message) -> EmbResult<()> {
        // Extract file path from payload
        let file_path = message.payload.get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EmbError::MessageQueue("Missing file_path in print start command".to_string()))?;
        
        // Request state transition to Preparing
        self.state_machine.transition_to(
            PrinterState::Preparing,
            TransitionReason::UserRequest,
        )?;
        
        // Load print job file
        self.print_controller.load_file(file_path).await?;
        
        // Start print job
        self.print_controller.start().await?;
        
        // Request state transition to Printing
        self.state_machine.transition_to(
            PrinterState::Printing,
            TransitionReason::OperationComplete,
        )?;
        
        log::info!("Print started: {}", file_path);
        Ok(())
    }
    
    /// Handle print pause command
    async fn handle_print_pause(&self, _message: &mut Message) -> EmbResult<()> {
        // Request state transition to Paused
        self.state_machine.transition_to(
            PrinterState::Paused,
            TransitionReason::UserRequest,
        )?;
        
        // Pause print job
        self.print_controller.pause().await?;
        
        log::info!("Print paused");
        Ok(())
    }
    
    /// Handle print resume command
    async fn handle_print_resume(&self, _message: &mut Message) -> EmbResult<()> {
        // Request state transition to Printing
        self.state_machine.transition_to(
            PrinterState::Printing,
            TransitionReason::UserRequest,
        )?;
        
        // Resume print job
        self.print_controller.resume().await?;
        
        log::info!("Print resumed");
        Ok(())
    }
    
    /// Handle print stop command
    async fn handle_print_stop(&self, _message: &mut Message) -> EmbResult<()> {
        // Request state transition to Idle
        self.state_machine.transition_to(
            PrinterState::Idle,
            TransitionReason::UserRequest,
        )?;
        
        // Cancel print job
        self.print_controller.stop().await;
        
        log::info!("Print stopped");
        Ok(())
    }
    
    /// Handle temperature set command
    async fn handle_temperature_set(&self, message: &mut Message) -> EmbResult<()> {
        // Extract heater and temperature from payload
        let heater = message.payload.get("heater")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EmbError::MessageQueue("Missing heater in temperature set command".to_string()))?;
        
        let temperature = message.payload.get("temperature")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| EmbError::MessageQueue("Missing temperature in temperature set command".to_string()))?;
        
        // Update temperature
        self.device_state.update_temperature(heater.to_string(), temperature as f32).await;
        
        log::info!("Temperature set: {} = {}", heater, temperature);
        Ok(())
    }
    
    /// Handle move command
    async fn handle_move_command(&self, message: &mut Message) -> EmbResult<()> {
        // Extract position from payload
        let x = message.payload.get("x").and_then(|v| v.as_f64()).map(|v| v as f32);
        let y = message.payload.get("y").and_then(|v| v.as_f64()).map(|v| v as f32);
        let z = message.payload.get("z").and_then(|v| v.as_f64()).map(|v| v as f32);
        let e = message.payload.get("e").and_then(|v| v.as_f64()).map(|v| v as f32);
        
        // Get current position
        let current_pos = self.device_state.get_position().await;
        
        // Create new position
        let new_pos = Position {
            x: x.unwrap_or(current_pos.x),
            y: y.unwrap_or(current_pos.y),
            z: z.unwrap_or(current_pos.z),
            e: e.unwrap_or(current_pos.e),
        };
        
        // Update position
        self.device_state.update_position(new_pos).await;
        
        log::info!("Move command: X={}, Y={}, Z={}, E={}", new_pos.x, new_pos.y, new_pos.z, new_pos.e);
        Ok(())
    }
    
    /// Handle home command
    async fn handle_home_command(&self, message: &mut Message) -> EmbResult<()> {
        // Extract axes to home from payload
        let axes = message.payload.get("axes")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
            .unwrap_or_else(|| vec!["x", "y", "z"]);
        
        // Home axes (set position to 0)
        let current_pos = self.device_state.get_position().await;
        let new_pos = Position {
            x: if axes.contains(&"x") { 0.0 } else { current_pos.x },
            y: if axes.contains(&"y") { 0.0 } else { current_pos.y },
            z: if axes.contains(&"z") { 0.0 } else { current_pos.z },
            e: current_pos.e,
        };
        
        self.device_state.update_position(new_pos).await;
        
        log::info!("Home command: axes = {:?}", axes);
        Ok(())
    }
}

#[async_trait]
impl MessageHandler for CommandHandler {
    async fn handle(&self, message: &mut Message) -> EmbResult<()> {
        match message.message_type {
            MessageType::PrintStart => self.handle_print_start(message).await,
            MessageType::PrintPause => self.handle_print_pause(message).await,
            MessageType::PrintResume => self.handle_print_resume(message).await,
            MessageType::PrintStop => self.handle_print_stop(message).await,
            MessageType::TemperatureSet => self.handle_temperature_set(message).await,
            MessageType::MoveCommand => self.handle_move_command(message).await,
            MessageType::HomeCommand => self.handle_home_command(message).await,
            _ => Err(EmbError::MessageQueue(format!("Unsupported message type: {:?}", message.message_type))),
        }
    }
    
    fn name(&self) -> &str {
        "CommandHandler"
    }
}