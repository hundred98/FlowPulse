//! PID auto-tune module
//!
//! This module provides PID auto-tune communication with the lower machine.
//!
//! # Overview
//!
//! PID auto-tuning is performed by the lower machine (MCU) for precise timing control.
//! The upper machine (PC) sends commands and receives results.
//!
//! # Communication Flow
//!
//! ```text
//! Upper Machine                          Lower Machine
//!      |                                      |
//!      |------- START (0x10) --------------->|
//!      |<------ ACK (0x15) ------------------|
//!      |                                      |
//!      |<------ PROGRESS (0x13) -------------|  (every 1 second)
//!      |<------ PROGRESS (0x13) -------------|
//!      |              ...                     |
//!      |                                      |
//!      |<------ COMPLETE (0x14) -------------|
//!      |                                       |
//!      |------- APPLY (0x12) ---------------->|
//!      |<------ ACK (0x15) -------------------|
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use emb_public::temperature::pid_tune::{PidTuneProtocol, PidParams};
//!
//! // Build START frame
//! let payload = PidTuneProtocol::build_start_payload(1, 200.0, 6);
//! send_frame(FrameType::Temperature, &payload);
//!
//! // Parse PROGRESS frame
//! let progress = PidTuneProtocol::parse_progress(&payload)?;
//! println!("Progress: {}%", progress.percent());
//!
//! // Parse COMPLETE frame
//! let result = PidTuneProtocol::parse_complete(&payload, "hotend")?;
//! if result.success {
//!     println!("New PID: Kp={:.3}, Ki={:.3}, Kd={:.3}",
//!         result.new_pid.kp, result.new_pid.ki, result.new_pid.kd);
//! }
//! ```

pub mod protocol;
pub mod types;

pub use protocol::{PidTuneProtocol, PidTuneSubType};
pub use types::{
    PidParams, PidTuneResult, TuneErrorCode, TunePhase, TuneProgress,
};
