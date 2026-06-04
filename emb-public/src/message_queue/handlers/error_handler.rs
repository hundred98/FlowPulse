//! Error handler for handling errors and retries

use crate::{EmbResult, EmbError};
use crate::safety::SafetyController;
use crate::state_machine::{StateMachine, PrinterState, TransitionReason};
use super::{MessageHandler, Message, MessageType, MessageStatus};
use async_trait::async_trait;
use std::sync::Arc;
use chrono::Utc;

/// Error handler for handling errors and retries
pub struct ErrorHandler {
    /// Safety controller
    safety_controller: Arc<SafetyController>,
    
    /// State machine
    state_machine: Arc<StateMachine>,
}

impl ErrorHandler {
    /// Create a new error handler
    pub fn new(
        safety_controller: Arc<SafetyController>,
        state_machine: Arc<StateMachine>,
    ) -> Self {
        Self {
            safety_controller,
            state_machine,
        }
    }
    
    /// Handle error message
    async fn handle_error(&self, message: &mut Message) -> EmbResult<()> {
        // Extract error details from payload
        let error_type = message.payload.get("error_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        
        let error_message = message.payload.get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error");
        
        let severity = message.payload.get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("error");
        
        log::error!("Error received: type={}, message={}, severity={}", error_type, error_message, severity);
        
        // Check if emergency stop is needed
        if severity == "critical" {
            // Trigger emergency stop
            self.safety_controller.handle_emergency_stop().await?;
            
            // Transition to Error state
            self.state_machine.transition_to(
                PrinterState::Error,
                TransitionReason::ErrorOccurred,
            )?;
            
            log::warn!("Emergency stop triggered due to critical error");
        }
        
        // Update message payload with error handling result
        message.payload = serde_json::json!({
            "error_type": error_type,
            "message": error_message,
            "severity": severity,
            "handled": true,
            "timestamp": Utc::now().to_rfc3339(),
        });
        
        Ok(())
    }
    
    /// Handle hardware error
    async fn handle_hardware_error(&self, message: &mut Message) -> EmbResult<()> {
        // Extract hardware error details
        let component = message.payload.get("component")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        
        let error_code = message.payload.get("error_code")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        
        log::error!("Hardware error: component={}, error_code={}", component, error_code);
        
        // Check safety violations
        if self.safety_controller.has_safety_violation().await {
            // Trigger emergency stop
            self.safety_controller.handle_emergency_stop().await?;
            
            // Transition to Error state
            self.state_machine.transition_to(
                PrinterState::Error,
                TransitionReason::ErrorOccurred,
            )?;
        }
        
        // Update message payload
        message.payload = serde_json::json!({
            "component": component,
            "error_code": error_code,
            "handled": true,
            "safety_violation": self.safety_controller.has_safety_violation().await,
        });
        
        Ok(())
    }
    
    /// Handle retry logic
    async fn handle_retry(&self, message: &mut Message) -> EmbResult<()> {
        // Check if message can be retried
        if message.can_retry() {
            log::info!("Retrying message: id={}, retry_count={}", message.id, message.retry_count);
            
            // Increment retry count
            message.increment_retry();
            
            // Reset status to pending
            message.status = MessageStatus::Pending;
            message.started_at = None;
            message.completed_at = None;
            
            // Update payload with retry information
            message.payload = serde_json::json!({
                "original_payload": message.payload.clone(),
                "retry_count": message.retry_count,
                "max_retries": message.max_retries,
                "retrying": true,
            });
        } else {
            log::warn!("Message cannot be retried: id={}, max_retries reached", message.id);
            
            // Mark as failed permanently
            message.mark_failed();
            
            message.payload = serde_json::json!({
                "original_payload": message.payload.clone(),
                "retry_count": message.retry_count,
                "max_retries": message.max_retries,
                "retrying": false,
                "failed_permanently": true,
            });
        }
        
        Ok(())
    }
}

#[async_trait]
impl MessageHandler for ErrorHandler {
    async fn handle(&self, message: &mut Message) -> EmbResult<()> {
        match message.message_type {
            MessageType::PrintError => self.handle_error(message).await,
            MessageType::HardwareError => self.handle_hardware_error(message).await,
            MessageType::SystemEvent => {
                // Check if this is a retry request
                if message.payload.get("retry").and_then(|v| v.as_bool()).unwrap_or(false) {
                    self.handle_retry(message).await
                } else {
                    self.handle_error(message).await
                }
            },
            _ => Err(EmbError::MessageQueue(format!("Unsupported message type: {:?}", message.message_type))),
        }
    }
    
    fn name(&self) -> &str {
        "ErrorHandler"
    }
}