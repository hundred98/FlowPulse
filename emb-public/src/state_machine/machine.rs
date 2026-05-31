//! Core state machine implementation

use crate::{EmbError, EmbResult, PrinterEvent, SyncEventPublisher, EventListener};
use chrono::Utc;
use serde_json;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use super::types::{PrinterState, TransitionReason, StateTransition, StateMachineConfig};

/// Core state machine for 3D printer control
pub struct StateMachine {
    /// Current printer state
    current_state: Arc<Mutex<PrinterState>>,
    /// State transition history
    transition_history: Arc<Mutex<Vec<StateTransition>>>,
    /// Event publisher
    event_publisher: Arc<Mutex<SyncEventPublisher>>,
    /// Configuration
    config: StateMachineConfig,
    /// State machine ID
    id: Uuid,
}

impl StateMachine {
    /// Create a new state machine
    pub fn new(config: StateMachineConfig) -> Self {
        Self {
            current_state: Arc::new(Mutex::new(PrinterState::Idle)),
            transition_history: Arc::new(Mutex::new(Vec::new())),
            event_publisher: Arc::new(Mutex::new(SyncEventPublisher::new())),
            config,
            id: Uuid::new_v4(),
        }
    }
    
    /// Get the current state
    pub fn get_state(&self) -> PrinterState {
        self.current_state.lock().unwrap().clone()
    }
    
    /// Request a state transition
    pub fn transition_to(&self, new_state: PrinterState, reason: TransitionReason) -> EmbResult<()> {
        let current_state = self.get_state();
        
        // Validate transition
        if !self.is_valid_transition(&current_state, &new_state) {
            return Err(EmbError::InvalidTransition {
                from: format!("{:?}", current_state),
                to: format!("{:?}", new_state),
            });
        }
        
        // Create transition record
        let transition = StateTransition {
            id: Uuid::new_v4(),
            from_state: current_state.clone(),
            to_state: new_state.clone(),
            reason: reason.clone(),
            timestamp: Utc::now(),
            data: None,
        };
        
        // Update state
        {
            let mut state_guard = self.current_state.lock().unwrap();
            *state_guard = new_state.clone();
        }
        
        // Add to history
        {
            let mut history_guard = self.transition_history.lock().unwrap();
            history_guard.push(transition.clone());
            
            // Limit history size
            if history_guard.len() > self.config.max_history_size {
                history_guard.remove(0);
            }
        }
        
        // Publish state change event
        let event = PrinterEvent::state_change(current_state.clone(), new_state.clone())
            .with_data(serde_json::json!({
                "transition_id": transition.id,
                "reason": reason,
                "timestamp": transition.timestamp
            }));
        
        self.event_publisher.lock().unwrap().publish(event);
        
        log::info!("State transition: {:?} -> {:?} (reason: {:?})", 
                  current_state, new_state, reason);
        
        Ok(())
    }
    
    /// Check if a transition is valid
    pub fn is_valid_transition(&self, from: &PrinterState, to: &PrinterState) -> bool {
        use PrinterState::*;
        
        match (from, to) {
            // Idle can go to Preparing or Error
            (Idle, Preparing) | (Idle, Error) => true,
            
            // Preparing can go to Printing, Idle (cancel), or Error
            (Preparing, Printing) | (Preparing, Idle) | (Preparing, Error) => true,
            
            // Printing can go to Paused, Complete, or Error
            (Printing, Paused) | (Printing, Complete) | (Printing, Error) => true,
            
            // Paused can go to Printing (resume), Idle (cancel), or Error
            (Paused, Printing) | (Paused, Idle) | (Paused, Error) => true,
            
            // Complete can go to Idle
            (Complete, Idle) => true,
            
            // Error can go to Idle (reset)
            (Error, Idle) => true,
            
            // Same state is not a transition
            (from, to) if from == to => false,
            
            // All other transitions are invalid
            _ => false,
        }
    }
    
    /// Get transition history
    pub fn get_transition_history(&self) -> Vec<StateTransition> {
        self.transition_history.lock().unwrap().clone()
    }
    
    /// Get the latest transition
    pub fn get_latest_transition(&self) -> Option<StateTransition> {
        let history = self.transition_history.lock().unwrap();
        history.last().cloned()
    }
    
    /// Add an event listener
    pub fn add_event_listener(&self, listener: Box<dyn EventListener>) {
        self.event_publisher.lock().unwrap().add_listener(listener);
    }
    
    /// Reset to idle state
    pub fn reset(&self) -> EmbResult<()> {
        self.transition_to(PrinterState::Idle, TransitionReason::SystemInitiated)
    }
    
    /// Check if the printer is in an operational state
    pub fn is_operational(&self) -> bool {
        matches!(self.get_state(), PrinterState::Idle | PrinterState::Preparing | PrinterState::Printing | PrinterState::Paused)
    }
    
    /// Check if the printer is in an error state
    pub fn is_error(&self) -> bool {
        matches!(self.get_state(), PrinterState::Error)
    }
    
    /// Check if the printer can start printing
    pub fn can_start_print(&self) -> bool {
        matches!(self.get_state(), PrinterState::Idle)
    }
    
    /// Check if the printer can pause
    pub fn can_pause(&self) -> bool {
        matches!(self.get_state(), PrinterState::Printing)
    }
    
    /// Check if the printer can resume
    pub fn can_resume(&self) -> bool {
        matches!(self.get_state(), PrinterState::Paused)
    }
    
    /// Check if the printer can cancel
    pub fn can_cancel(&self) -> bool {
        matches!(self.get_state(), PrinterState::Preparing | PrinterState::Printing | PrinterState::Paused)
    }
    
    /// Get state machine ID
    pub fn id(&self) -> Uuid {
        self.id
    }
    
    /// Get configuration
    pub fn config(&self) -> &StateMachineConfig {
        &self.config
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new(StateMachineConfig::default())
    }
}

/// Convenience methods for common state transitions
impl StateMachine {
    /// Start preparing for print
    pub fn start_preparing(&self) -> EmbResult<()> {
        self.transition_to(PrinterState::Preparing, TransitionReason::UserRequest)
    }
    
    /// Start printing
    pub fn start_printing(&self) -> EmbResult<()> {
        self.transition_to(PrinterState::Printing, TransitionReason::UserRequest)
    }
    
    /// Pause printing
    pub fn pause(&self) -> EmbResult<()> {
        self.transition_to(PrinterState::Paused, TransitionReason::UserRequest)
    }
    
    /// Resume printing
    pub fn resume(&self) -> EmbResult<()> {
        self.transition_to(PrinterState::Printing, TransitionReason::UserRequest)
    }
    
    /// Complete printing
    pub fn complete(&self) -> EmbResult<()> {
        self.transition_to(PrinterState::Complete, TransitionReason::OperationComplete)
    }
    
    /// Cancel current operation
    pub fn cancel(&self) -> EmbResult<()> {
        if matches!(self.get_state(), PrinterState::Preparing | PrinterState::Printing | PrinterState::Paused) {
            self.transition_to(PrinterState::Idle, TransitionReason::UserRequest)
        } else {
            Err(EmbError::StateMachine("Cannot cancel in current state".to_string()))
        }
    }
    
    /// Enter error state
    pub fn enter_error(&self, error_msg: String) -> EmbResult<()> {
        self.transition_to(PrinterState::Error, TransitionReason::Error(error_msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventKind;
    
    #[derive(Clone)]
    struct TestListener {
        events: Arc<Mutex<Vec<PrinterEvent>>>,
    }
    
    impl TestListener {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }
        
        fn get_events(&self) -> Vec<PrinterEvent> {
            self.events.lock().unwrap().clone()
        }
    }
    
    impl EventListener for TestListener {
        fn on_event(&self, event: &PrinterEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }
    
    #[test]
    fn test_valid_transitions() {
        let sm = StateMachine::default();
        
        assert!(sm.is_valid_transition(&PrinterState::Idle, &PrinterState::Preparing));
        assert!(sm.is_valid_transition(&PrinterState::Preparing, &PrinterState::Printing));
        assert!(sm.is_valid_transition(&PrinterState::Printing, &PrinterState::Paused));
        assert!(sm.is_valid_transition(&PrinterState::Paused, &PrinterState::Printing));
        assert!(sm.is_valid_transition(&PrinterState::Printing, &PrinterState::Complete));
        assert!(sm.is_valid_transition(&PrinterState::Complete, &PrinterState::Idle));
        assert!(sm.is_valid_transition(&PrinterState::Error, &PrinterState::Idle));
    }
    
    #[test]
    fn test_invalid_transitions() {
        let sm = StateMachine::default();
        
        assert!(!sm.is_valid_transition(&PrinterState::Idle, &PrinterState::Printing));
        assert!(!sm.is_valid_transition(&PrinterState::Complete, &PrinterState::Printing));
        assert!(!sm.is_valid_transition(&PrinterState::Error, &PrinterState::Printing));
    }
    
    #[test]
    fn test_state_transition() -> EmbResult<()> {
        let sm = StateMachine::default();
        let listener = TestListener::new();
        sm.add_event_listener(Box::new(listener.clone()));
        
        assert_eq!(sm.get_state(), PrinterState::Idle);
        
        sm.start_preparing()?;
        assert_eq!(sm.get_state(), PrinterState::Preparing);
        
        let events = listener.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, EventKind::StateChanged);
        
        Ok(())
    }
    
    #[test]
    fn test_convenience_methods() -> EmbResult<()> {
        let sm = StateMachine::default();
        
        assert!(sm.can_start_print());
        assert!(!sm.can_pause());
        assert!(!sm.can_resume());
        assert!(!sm.can_cancel());
        
        sm.start_preparing()?;
        assert!(!sm.can_start_print());
        assert!(!sm.can_pause());
        assert!(!sm.can_resume());
        assert!(sm.can_cancel());
        
        sm.start_printing()?;
        assert!(!sm.can_start_print());
        assert!(sm.can_pause());
        assert!(!sm.can_resume());
        assert!(sm.can_cancel());
        
        sm.pause()?;
        assert!(!sm.can_start_print());
        assert!(!sm.can_pause());
        assert!(sm.can_resume());
        assert!(sm.can_cancel());
        
        Ok(())
    }
}