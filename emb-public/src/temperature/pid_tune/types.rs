//! PID auto-tune types and data structures
//!
//! This module defines types used for PID auto-tune communication with the lower machine.

use serde::{Deserialize, Serialize};

/// PID parameters
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PidParams {
    /// Proportional gain
    pub kp: f32,
    
    /// Integral gain
    pub ki: f32,
    
    /// Derivative gain
    pub kd: f32,
}

impl PidParams {
    /// Create new PID parameters
    pub fn new(kp: f32, ki: f32, kd: f32) -> Self {
        Self { kp, ki, kd }
    }
    
    /// Check if parameters are valid
    pub fn is_valid(&self) -> bool {
        self.kp > 0.0 && self.ki >= 0.0 && self.kd >= 0.0
    }
}

impl Default for PidParams {
    fn default() -> Self {
        Self {
            kp: 1.0,
            ki: 0.0,
            kd: 0.0,
        }
    }
}

/// Tuning phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TunePhase {
    /// Idle
    Idle = 0,
    
    /// Heating to target temperature
    Heating = 1,
    
    /// Measuring oscillation
    Measuring = 2,
    
    /// Tuning complete
    Complete = 3,
    
    /// Tuning failed
    Failed = 4,
}

impl Default for TunePhase {
    fn default() -> Self {
        Self::Idle
    }
}

impl From<u8> for TunePhase {
    fn from(value: u8) -> Self {
        match value {
            0 => TunePhase::Idle,
            1 => TunePhase::Heating,
            2 => TunePhase::Measuring,
            3 => TunePhase::Complete,
            4 => TunePhase::Failed,
            _ => TunePhase::Idle,
        }
    }
}

/// Tuning progress information (received from lower machine)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuneProgress {
    /// Heater ID (0=bed, 1=hotend)
    pub heater_id: u8,
    
    /// Current phase
    pub phase: TunePhase,
    
    /// Current cycle (1-based)
    pub current_cycle: u8,
    
    /// Total cycles
    pub total_cycles: u8,
    
    /// Current temperature (°C)
    pub current_temp: f32,
    
    /// Current output power (0.0-1.0)
    pub output_power: f32,
}

impl TuneProgress {
    /// Get progress percentage (0-100)
    pub fn percent(&self) -> f32 {
        if self.total_cycles == 0 {
            return 0.0;
        }
        (self.current_cycle as f32 / self.total_cycles as f32) * 100.0
    }
}

/// Tuning result (received from lower machine)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PidTuneResult {
    /// Heater ID (0=bed, 1=hotend)
    pub heater_id: u8,
    
    /// Heater name
    pub heater_name: String,
    
    /// Whether tuning succeeded
    pub success: bool,
    
    /// New PID parameters
    pub new_pid: PidParams,
    
    /// Ultimate gain (Ku) from relay feedback
    pub ultimate_gain: f32,
    
    /// Ultimate period (Tu) from relay feedback (seconds)
    pub ultimate_period: f32,
    
    /// Number of cycles completed
    pub cycles_completed: u8,
    
    /// Tuning duration (milliseconds)
    pub duration_ms: u32,
    
    /// Error code (0 = no error)
    pub error_code: u8,
}

impl PidTuneResult {
    /// Create a successful result
    pub fn success(
        heater_id: u8,
        heater_name: String,
        new_pid: PidParams,
        ultimate_gain: f32,
        ultimate_period: f32,
        cycles_completed: u8,
        duration_ms: u32,
    ) -> Self {
        Self {
            heater_id,
            heater_name,
            success: true,
            new_pid,
            ultimate_gain,
            ultimate_period,
            cycles_completed,
            duration_ms,
            error_code: 0,
        }
    }
    
    /// Create a failed result
    pub fn failed(heater_id: u8, heater_name: String, error_code: u8) -> Self {
        Self {
            heater_id,
            heater_name,
            success: false,
            new_pid: PidParams::default(),
            ultimate_gain: 0.0,
            ultimate_period: 0.0,
            cycles_completed: 0,
            duration_ms: 0,
            error_code,
        }
    }
}

/// Error codes for PID tuning
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuneErrorCode {
    /// No error
    None = 0x00,
    
    /// Timeout (exceeded 5 minutes)
    Timeout = 0x01,
    
    /// Temperature sensor fault
    SensorFault = 0x02,
    
    /// Already tuning
    AlreadyTuning = 0x03,
    
    /// Insufficient cycles
    InsufficientCycles = 0x04,
    
    /// Parameter calculation failed
    CalculationFailed = 0x05,
    
    /// Invalid heater ID
    InvalidHeaterId = 0x06,
    
    /// Target temperature out of range
    TempOutOfRange = 0x07,
}

impl From<u8> for TuneErrorCode {
    fn from(value: u8) -> Self {
        match value {
            0x00 => TuneErrorCode::None,
            0x01 => TuneErrorCode::Timeout,
            0x02 => TuneErrorCode::SensorFault,
            0x03 => TuneErrorCode::AlreadyTuning,
            0x04 => TuneErrorCode::InsufficientCycles,
            0x05 => TuneErrorCode::CalculationFailed,
            0x06 => TuneErrorCode::InvalidHeaterId,
            0x07 => TuneErrorCode::TempOutOfRange,
            _ => TuneErrorCode::None,
        }
    }
}

impl TuneErrorCode {
    /// Get error message
    pub fn message(&self) -> &'static str {
        match self {
            TuneErrorCode::None => "No error",
            TuneErrorCode::Timeout => "Tuning timeout (exceeded 5 minutes)",
            TuneErrorCode::SensorFault => "Temperature sensor fault",
            TuneErrorCode::AlreadyTuning => "Already tuning",
            TuneErrorCode::InsufficientCycles => "Insufficient cycles for calculation",
            TuneErrorCode::CalculationFailed => "Parameter calculation failed",
            TuneErrorCode::InvalidHeaterId => "Invalid heater ID",
            TuneErrorCode::TempOutOfRange => "Target temperature out of range",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pid_params_valid() {
        let params = PidParams::new(10.0, 1.0, 5.0);
        assert!(params.is_valid());
        
        let invalid = PidParams::new(-1.0, 1.0, 5.0);
        assert!(!invalid.is_valid());
    }
    
    #[test]
    fn test_tune_phase_from_u8() {
        assert_eq!(TunePhase::from(0), TunePhase::Idle);
        assert_eq!(TunePhase::from(1), TunePhase::Heating);
        assert_eq!(TunePhase::from(2), TunePhase::Measuring);
        assert_eq!(TunePhase::from(3), TunePhase::Complete);
        assert_eq!(TunePhase::from(4), TunePhase::Failed);
    }
    
    #[test]
    fn test_tune_progress_percent() {
        let progress = TuneProgress {
            heater_id: 1,
            phase: TunePhase::Measuring,
            current_cycle: 3,
            total_cycles: 6,
            current_temp: 200.0,
            output_power: 0.5,
        };
        
        assert_eq!(progress.percent(), 50.0);
    }
    
    #[test]
    fn test_error_code_message() {
        assert_eq!(TuneErrorCode::Timeout.message(), "Tuning timeout (exceeded 5 minutes)");
        assert_eq!(TuneErrorCode::SensorFault.message(), "Temperature sensor fault");
    }
}
