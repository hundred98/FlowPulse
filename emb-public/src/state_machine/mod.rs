//! Core state machine for 3D printer control
//!
//! This module provides state machine capabilities for managing printer states.

pub mod types;
pub mod machine;

pub use types::{
    PrinterState, TransitionReason, StateTransition, StateMachineConfig,
};
pub use machine::StateMachine;