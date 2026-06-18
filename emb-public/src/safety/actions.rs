//! Safety action executor
//!
//! This module handles the execution of safety actions such as turning off heaters,
//! pausing prints, disabling motors, and triggering emergency stops.
//! It abstracts the coordination between printing, temperature, GPIO, and core subsystems.

use super::types::{SafetyAction, SafetyCheckResult};
use crate::common::{EventPublisher, EventKind, EventSeverity, PrinterEvent};
use std::sync::Arc;

/// Optional references to subsystems that safety actions may need to control.
///
/// All fields are `Option` to allow the safety module to be initialized
/// before all other subsystems are ready.
pub struct SafetyActionExecutor {
    event_publisher: Arc<dyn EventPublisher>,
    temperature_manager: Option<Arc<crate::temperature::TemperatureManager>>,
    print_controller: Option<Arc<crate::print_control::PrintController>>,
    core_client: Option<Arc<crate::core_client::CoreSocketClient>>,
    state_machine: Option<Arc<crate::state_machine::StateMachine>>,
}

impl SafetyActionExecutor {
    /// Create a new action executor
    pub fn new(event_publisher: Arc<dyn EventPublisher>) -> Self {
        Self {
            event_publisher,
            temperature_manager: None,
            print_controller: None,
            core_client: None,
            state_machine: None,
        }
    }

    // ---- Dependency injection ----

    pub fn with_temperature_manager(mut self, mgr: Arc<crate::temperature::TemperatureManager>) -> Self {
        self.temperature_manager = Some(mgr);
        self
    }

    pub fn with_print_controller(mut self, ctrl: Arc<crate::print_control::PrintController>) -> Self {
        self.print_controller = Some(ctrl);
        self
    }

    pub fn with_core_client(mut self, client: Arc<crate::core_client::CoreSocketClient>) -> Self {
        self.core_client = Some(client);
        self
    }

    pub fn with_state_machine(mut self, sm: Arc<crate::state_machine::StateMachine>) -> Self {
        self.state_machine = Some(sm);
        self
    }

    // ---- Action execution ----

    /// Execute the appropriate action for a safety check result.
    pub async fn execute(&self, result: &SafetyCheckResult) {
        match result.action {
            SafetyAction::None => {}
            SafetyAction::LogWarning => {
                self.publish_event(result, EventSeverity::Warning);
            }
            SafetyAction::TurnOffHeater => {
                self.execute_turn_off_heater(result).await;
            }
            SafetyAction::PausePrint => {
                self.execute_pause_print(result).await;
            }
            SafetyAction::DisableMotors => {
                self.execute_disable_motors(result).await;
            }
            SafetyAction::EmergencyStop => {
                self.execute_emergency_stop(result).await;
            }
        }
    }

    /// Execute actions for multiple results (most severe first)
    pub async fn execute_all(&self, results: &[SafetyCheckResult]) {
        let mut sorted: Vec<_> = results.iter().collect();
        sorted.sort_by(|a, b| b.cmp(a)); // most severe first
        for r in sorted {
            if r.needs_action() {
                tracing::warn!("Safety action: {:?} | {}", r.action, r.message);
                self.execute(r).await;
            }
        }
    }

    // ---- Private implementations ----

    async fn execute_turn_off_heater(&self, result: &SafetyCheckResult) {
        let heater = result.source.as_str();
        if let Some(ref tm) = self.temperature_manager {
            if let Err(e) = tm.set_target(heater, 0.0).await {
                tracing::error!("Failed to turn off heater '{}': {}", heater, e);
            }
        } else {
            tracing::warn!("TemperatureManager not available, cannot turn off heater '{}'", heater);
        }
        self.publish_event(result, EventSeverity::Error);
    }

    async fn execute_pause_print(&self, result: &SafetyCheckResult) {
        if let Some(ref pc) = self.print_controller {
            if let Err(e) = pc.pause().await {
                tracing::error!("Failed to pause print: {}", e);
            }
        } else {
            tracing::warn!("PrintController not available, cannot pause print");
        }
        self.publish_event(result, EventSeverity::Error);
    }

    async fn execute_disable_motors(&self, result: &SafetyCheckResult) {
        if let Some(ref client) = self.core_client {
            if let Err(e) = client.motion_execute_m_command(emb_api::MCommand::MotorDisableAll).await {
                tracing::error!("Failed to disable motors: {}", e);
            }
        } else {
            tracing::warn!("CoreClient not available, cannot disable motors");
        }
        self.publish_event(result, EventSeverity::Critical);
    }

    async fn execute_emergency_stop(&self, result: &SafetyCheckResult) {
        tracing::error!("🚨 Emergency stop triggered: {}", result.message);

        // 1. Send M112 to core server → MCU sched_shutdown (stops motion + disables heaters)
        if let Some(ref client) = self.core_client {
            if let Err(e) = client.motion_execute_m_command(emb_api::MCommand::EmergencyStop).await {
                tracing::error!("Failed to send EmergencyStop command: {}", e);
            }
        }

        // 2. Transition state machine to Error
        if let Some(ref sm) = self.state_machine {
            if let Err(e) = sm.transition_to(
                crate::state_machine::PrinterState::Error,
                crate::state_machine::TransitionReason::Error(result.message.clone()),
            ) {
                tracing::error!("Failed to transition to Error state during E-stop: {}", e);
            }
        }

        // 3. Publish critical event
        self.publish_event_with(result, EventSeverity::Critical, "Emergency stop executed");
    }

    fn publish_event(&self, result: &SafetyCheckResult, severity: EventSeverity) {
        let _ = self.event_publisher.publish(
            PrinterEvent::new(
                EventKind::SafetyWarning,
                format!("safety/{}", result.source),
                result.message.clone(),
            )
            .with_severity(severity),
        );
    }

    fn publish_event_with(&self, result: &SafetyCheckResult, severity: EventSeverity, suffix: &str) {
        let msg = format!("{} — {}", result.message, suffix);
        let _ = self.event_publisher.publish(
            PrinterEvent::new(
                EventKind::SafetyWarning,
                format!("safety/{}", result.source),
                msg,
            )
            .with_severity(severity),
        );
    }
}

