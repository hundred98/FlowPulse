//! State machine types and definitions

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// 3D Printer states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PrinterState {
    /// System is idle and ready for commands
    Idle,
    /// Preparing for print (loading file, heating, etc.)
    Preparing,
    /// Currently printing
    Printing,
    /// Print is paused
    Paused,
    /// Print completed successfully
    Complete,
    /// Error state
    Error,
}

impl fmt::Display for PrinterState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrinterState::Idle => write!(f, "Idle"),
            PrinterState::Preparing => write!(f, "Preparing"),
            PrinterState::Printing => write!(f, "Printing"),
            PrinterState::Paused => write!(f, "Paused"),
            PrinterState::Complete => write!(f, "Complete"),
            PrinterState::Error => write!(f, "Error"),
        }
    }
}

/// State transition reasons
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransitionReason {
    /// User requested transition
    UserRequest,
    /// System initiated transition
    SystemInitiated,
    /// Error occurred
    Error(String),
    /// Operation completed
    OperationComplete,
    /// Hardware event
    HardwareEvent,
}

/// State transition record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransition {
    /// Unique transition ID
    pub id: Uuid,
    /// Previous state
    pub from_state: PrinterState,
    /// New state
    pub to_state: PrinterState,
    /// Transition reason
    pub reason: TransitionReason,
    /// Transition timestamp
    pub timestamp: DateTime<Utc>,
    /// Optional transition data
    pub data: Option<serde_json::Value>,
}

/// State machine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineConfig {
    /// Enable automatic state persistence
    pub enable_persistence: bool,
    /// Maximum number of state transitions to keep in history
    pub max_history_size: usize,
    /// Timeout for state transitions (milliseconds)
    pub transition_timeout_ms: u64,
}

impl Default for StateMachineConfig {
    fn default() -> Self {
        Self {
            enable_persistence: true,
            max_history_size: 1000,
            transition_timeout_ms: 5000,
        }
    }
}

