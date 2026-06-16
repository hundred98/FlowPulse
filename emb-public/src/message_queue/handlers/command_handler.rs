//! Command handler for processing printer commands

use crate::{EmbResult, EmbError};
use crate::state::{DeviceStateManager, Position};
use crate::state_machine::{StateMachine, PrinterState, TransitionReason};
use crate::print_control::PrintController;
use crate::temperature::TemperatureManager;
use crate::message_queue::{MessageHandler, Message, MessageType};
use crate::CoreSocketClient;
use crate::gcode::{GCodeParser, CommandKind, MotionCommand};
use async_trait::async_trait;
use std::sync::Arc;

/// Command handler for processing printer commands
pub struct CommandHandler {
    /// Core socket client for communicating with emb-core-server
    core_client: Arc<CoreSocketClient>,

    /// Device state manager
    device_state: Arc<DeviceStateManager>,

    /// State machine
    state_machine: Arc<StateMachine>,

    /// Print controller
    print_controller: Arc<PrintController>,

    /// Temperature manager
    temperature_manager: Arc<TemperatureManager>,
}

impl CommandHandler {
    /// Create a new command handler
    pub fn new(
        core_client: Arc<CoreSocketClient>,
        device_state: Arc<DeviceStateManager>,
        state_machine: Arc<StateMachine>,
        print_controller: Arc<PrintController>,
        temperature_manager: Arc<TemperatureManager>,
    ) -> Self {
        Self {
            core_client,
            device_state,
            state_machine,
            print_controller,
            temperature_manager,
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
        // Cancel any ongoing temperature wait (M109/M190)
        self.temperature_manager.cancel_wait().await;

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
        // Cancel any ongoing temperature wait (M109/M190)
        self.temperature_manager.cancel_wait().await;

        // Request state transition to Idle
        self.state_machine.transition_to(
            PrinterState::Idle,
            TransitionReason::UserRequest,
        )?;
        
        // Cancel print job
        let _ = self.print_controller.stop().await;
        
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

        // Set temperature using TemperatureManager
        self.temperature_manager.set_target(heater, temperature as f32).await?;

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
    
    /// Handle PID tune start command
    async fn handle_pid_tune_start(&self, message: &mut Message) -> EmbResult<()> {
        // Extract heater from payload
        let heater = message.payload.get("heater")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EmbError::MessageQueue("Missing heater in PID tune command".to_string()))?;
        
        // Extract target temperature (optional, defaults based on heater)
        let target_temp = message.payload.get("target_temp")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or_else(|| match heater {
                "hotend" => 200.0,
                "bed" => 60.0,
                _ => 200.0,
            });
        
        // Extract cycles (optional, default 6)
        let cycles = message.payload.get("cycles")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8)
            .unwrap_or(6);
        
        // Start PID tuning
        self.temperature_manager.start_pid_tune(heater, target_temp, cycles).await?;
        
        log::info!("PID tuning started: heater={}, target={}°C", heater, target_temp);
        Ok(())
    }
    
    /// Handle PID tune cancel command
    async fn handle_pid_tune_cancel(&self, _message: &mut Message) -> EmbResult<()> {
        self.temperature_manager.cancel_pid_tune().await?;
        log::info!("PID tuning cancelled");
        Ok(())
    }
    
    /// Handle PID tune progress query
    async fn handle_pid_tune_progress(&self, message: &mut Message) -> EmbResult<()> {
        if let Some(progress) = self.temperature_manager.get_tune_progress().await {
            message.payload = serde_json::to_value(progress)
                .map_err(|e| EmbError::MessageQueue(format!("Failed to serialize progress: {}", e)))?;
        } else {
            message.payload = serde_json::json!({
                "tuning": false
            });
        }
        Ok(())
    }
    
    /// Handle PID tune result query
    async fn handle_pid_tune_result(&self, message: &mut Message) -> EmbResult<()> {
        if let Some(result) = self.temperature_manager.get_tune_result().await {
            message.payload = serde_json::to_value(result)
                .map_err(|e| EmbError::MessageQueue(format!("Failed to serialize result: {}", e)))?;
        } else {
            message.payload = serde_json::json!({
                "available": false
            });
        }
        Ok(())
    }
    
    /// Handle PID tune apply command
    async fn handle_pid_tune_apply(&self, _message: &mut Message) -> EmbResult<()> {
        // Apply the last tune result
        self.temperature_manager.apply_tune_result().await?;
        
        log::info!("PID parameters applied");
        Ok(())
    }
    
    /// Handle raw G-code line
    async fn handle_gcode_line(&self, message: &mut Message) -> EmbResult<()> {
        let gcode_str = message.payload.get("gcode")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EmbError::MessageQueue("Missing gcode in GcodeLine message".to_string()))?;
        
        log::info!("G-code line: {}", gcode_str);
        
        let parsed = GCodeParser::parse_line(gcode_str, 0)
            .ok_or_else(|| EmbError::MessageQueue(format!("Failed to parse G-code: {}", gcode_str)))?;
        
        match parsed.kind {
            CommandKind::Motion(motion_cmd) => {
                match motion_cmd {
                    MotionCommand::LinearMove { x, y, z, e, f, is_rapid } => {
                        let cmd = if is_rapid { "G0" } else { "G1" };
                        
                        // Non-print-mode homed check (server also checks, but do early check here)
                        let print_mode = message.payload.get("print_mode")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        
                        if !print_mode {
                            match self.core_client.motion_query_homed().await {
                                Ok(homed) if homed != 0b111 => {
                                    return Err(EmbError::MessageQueue(format!(
                                        "Cannot move: axes not homed (homed_axes={:#04b}). Use G28 first.", homed
                                    )));
                                }
                                Err(e) => {
                                    return Err(EmbError::MessageQueue(format!("Homed query failed: {}", e)));
                                }
                                _ => {}
                            }
                        }
                        
                        self.core_client.motion_dispatch(cmd, x, y, z, e, f).await
                            .map_err(|e| EmbError::MessageQueue(format!("Motion dispatch failed: {}", e)))
                            .map(|_| ())?;
                    }
                    MotionCommand::ArcMove { x, y, z, e, f, i, j, is_cw } => {
                        let cmd = if is_cw { "G2" } else { "G3" };
                        let arc = Some(emb_api::ArcParamsApi { i, j, direction: if is_cw { 0 } else { 1 } });
                        
                        // Homed check for arc moves
                        match self.core_client.motion_query_homed().await {
                            Ok(homed) if homed != 0b111 => {
                                return Err(EmbError::MessageQueue(format!(
                                    "Cannot move: axes not homed (homed_axes={:#04b}). Use G28 first.", homed
                                )));
                            }
                            Err(e) => {
                                return Err(EmbError::MessageQueue(format!("Homed query failed: {}", e)));
                            }
                            _ => {}
                        }
                        
                        self.core_client.motion_dispatch_arc(&cmd, x, y, z, e, f, arc).await
                            .map_err(|e| EmbError::MessageQueue(format!("Arc dispatch failed: {}", e)))
                            .map(|_| ())?;
                    }
                    MotionCommand::Home { .. } => {
                        // G28
                        self.core_client.motion_dispatch("G28", None, None, None, None, None).await
                            .map_err(|e| EmbError::MessageQueue(format!("Home failed: {}", e)))
                            .map(|_| ())?;
                    }
                    MotionCommand::SetPosition { x, y, z, e } => {
                        self.core_client.motion_set_position(x, y, z, e).await
                            .map_err(|e| EmbError::MessageQueue(format!("SetPosition failed: {}", e)))?;
                    }
                    MotionCommand::AbsolutePositioning => {
                        self.core_client.motion_execute_m_command(emb_api::MCommand::AbsolutePositioning).await
                            .map_err(|e| EmbError::MessageQueue(e))?;
                    }
                    MotionCommand::RelativePositioning => {
                        self.core_client.motion_execute_m_command(emb_api::MCommand::RelativePositioning).await
                            .map_err(|e| EmbError::MessageQueue(e))?;
                    }
                    MotionCommand::Dwell { dwell_time_ms } => {
                        self.core_client.motion_dwell(dwell_time_ms).await
                            .map_err(|e| EmbError::MessageQueue(e))?;
                    }
                    _ => {
                        return Err(EmbError::MessageQueue(format!("Unsupported motion command: {:?}", motion_cmd)));
                    }
                }
            }
            CommandKind::Machine(m_cmd) => {
                self.core_client.motion_execute_m_command(m_cmd).await
                    .map_err(|e| EmbError::MessageQueue(e))?;
            }
            _ => {
                return Err(EmbError::MessageQueue(format!("Unsupported G-code command: {}", gcode_str)));
            }
        }
        
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
            MessageType::PidTuneStart => self.handle_pid_tune_start(message).await,
            MessageType::PidTuneCancel => self.handle_pid_tune_cancel(message).await,
            MessageType::PidTuneProgress => self.handle_pid_tune_progress(message).await,
            MessageType::PidTuneResult => self.handle_pid_tune_result(message).await,
            MessageType::PidTuneApply => self.handle_pid_tune_apply(message).await,
            MessageType::GcodeLine => self.handle_gcode_line(message).await,
            _ => Err(EmbError::MessageQueue(format!("Unsupported message type: {:?}", message.message_type))),
        }
    }
    
    fn name(&self) -> &str {
        "CommandHandler"
    }
}