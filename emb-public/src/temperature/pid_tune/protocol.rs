//! PID auto-tune protocol handling
//!
//! This module handles frame building and parsing for PID auto-tune communication.
//!
//! # Frame Format
//!
//! ```text
//! [SOF:1][LEN:1][TYPE:1][PAYLOAD:N][CRC8:1][EOF:1]
//! ```
//!
//! For PID tune, TYPE = 0x02 (TEMPERATURE), and PAYLOAD starts with SUB_TYPE.
//!
//! # Sub Types
//!
//! | Sub Type | Name              | Direction           |
//! |----------|-------------------|---------------------|
//! | 0x10     | PID_TUNE_START    | Upper -> Lower      |
//! | 0x11     | PID_TUNE_CANCEL   | Upper -> Lower      |
//! | 0x12     | PID_TUNE_APPLY    | Upper -> Lower      |
//! | 0x13     | PID_TUNE_PROGRESS | Lower -> Upper      |
//! | 0x14     | PID_TUNE_COMPLETE | Lower -> Upper      |
//! | 0x15     | PID_TUNE_ACK      | Lower -> Upper      |

use super::types::{PidParams, PidTuneResult, TunePhase, TuneProgress};
use crate::common::EmbError;

/// PID tune sub-frame types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidTuneSubType {
    /// Start tuning
    Start = 0x10,
    
    /// Cancel tuning
    Cancel = 0x11,
    
    /// Apply new parameters
    Apply = 0x12,
    
    /// Progress report
    Progress = 0x13,
    
    /// Tuning complete
    Complete = 0x14,
    
    /// Command acknowledgment
    Ack = 0x15,
}

impl From<u8> for PidTuneSubType {
    fn from(value: u8) -> Self {
        match value {
            0x10 => PidTuneSubType::Start,
            0x11 => PidTuneSubType::Cancel,
            0x12 => PidTuneSubType::Apply,
            0x13 => PidTuneSubType::Progress,
            0x14 => PidTuneSubType::Complete,
            0x15 => PidTuneSubType::Ack,
            _ => PidTuneSubType::Ack, // Default fallback
        }
    }
}

/// PID tune protocol handler
pub struct PidTuneProtocol;

impl PidTuneProtocol {
    /// Build START frame payload
    ///
    /// Format: [SUB_TYPE:1][HEATER_ID:1][TARGET_TEMP:2][CYCLES:1][OPTIONS:1]
    ///
    /// # Arguments
    /// * `heater_id` - 0=bed, 1=hotend
    /// * `target_temp` - Target temperature in °C
    /// * `cycles` - Number of tuning cycles (recommended: 6-8)
    ///
    /// # Returns
    /// Payload bytes (without SOF/LEN/TYPE/CRC8/EOF)
    pub fn build_start_payload(heater_id: u8, target_temp: f32, cycles: u8) -> Vec<u8> {
        let temp_raw = (target_temp * 10.0) as u16;
        
        vec![
            PidTuneSubType::Start as u8,
            heater_id,
            (temp_raw >> 8) as u8,           // High byte (big-endian)
            (temp_raw & 0xFF) as u8,          // Low byte
            cycles,
            0,                                // Options (reserved)
        ]
    }
    
    /// Build CANCEL frame payload
    ///
    /// Format: [SUB_TYPE:1][HEATER_ID:1]
    pub fn build_cancel_payload(heater_id: u8) -> Vec<u8> {
        vec![
            PidTuneSubType::Cancel as u8,
            heater_id,
        ]
    }
    
    /// Build APPLY frame payload
    ///
    /// Format: [SUB_TYPE:1][HEATER_ID:1][KP:4][KI:4][KD:4]
    ///
    /// PID values are IEEE 754 float in big-endian
    pub fn build_apply_payload(heater_id: u8, params: &PidParams) -> Vec<u8> {
        let mut payload = vec![
            PidTuneSubType::Apply as u8,
            heater_id,
        ];
        
        // Append PID values as big-endian floats
        payload.extend_from_slice(&Self::float_to_bytes(params.kp));
        payload.extend_from_slice(&Self::float_to_bytes(params.ki));
        payload.extend_from_slice(&Self::float_to_bytes(params.kd));
        
        payload
    }
    
    /// Parse PROGRESS frame payload
    ///
    /// Format: [SUB_TYPE:1][HEATER_ID:1][PHASE:1][CYCLE:1][TOTAL_CYCLES:1][CURRENT_TEMP:2][OUTPUT:1]
    pub fn parse_progress(payload: &[u8]) -> Result<TuneProgress, EmbError> {
        if payload.len() < 8 {
            return Err(EmbError::Protocol("Invalid PROGRESS payload length".to_string()));
        }
        
        let heater_id = payload[1];
        let phase = TunePhase::from(payload[2]);
        let current_cycle = payload[3];
        let total_cycles = payload[4];
        
        // Current temp (big-endian u16, value * 10)
        let temp_raw = ((payload[5] as u16) << 8) | (payload[6] as u16);
        let current_temp = temp_raw as f32 / 10.0;
        
        // Output power (value * 2, so divide by 2)
        let output_power = payload[7] as f32 / 200.0;
        
        Ok(TuneProgress {
            heater_id,
            phase,
            current_cycle,
            total_cycles,
            current_temp,
            output_power,
        })
    }
    
    /// Parse COMPLETE frame payload
    ///
    /// Format:
    /// [SUB_TYPE:1][HEATER_ID:1][SUCCESS:1][KP:4][KI:4][KD:4][CYCLES_DONE:1][KU:4][TU:4][DURATION:4][ERROR_CODE:1]
    pub fn parse_complete(payload: &[u8], heater_name: &str) -> Result<PidTuneResult, EmbError> {
        if payload.len() < 30 {
            return Err(EmbError::Protocol("Invalid COMPLETE payload length".to_string()));
        }
        
        let heater_id = payload[1];
        let success = payload[2] != 0;
        
        // Parse PID values (big-endian floats)
        let kp = Self::bytes_to_float(&payload[3..7]);
        let ki = Self::bytes_to_float(&payload[7..11]);
        let kd = Self::bytes_to_float(&payload[11..15]);
        
        let cycles_completed = payload[15];
        
        // Parse Ku, Tu, duration (big-endian)
        let ultimate_gain = Self::bytes_to_float(&payload[16..20]);
        let ultimate_period = Self::bytes_to_float(&payload[20..24]);
        
        let duration_raw = ((payload[24] as u32) << 24)
            | ((payload[25] as u32) << 16)
            | ((payload[26] as u32) << 8)
            | (payload[27] as u32);
        
        let error_code = payload[28];
        
        Ok(PidTuneResult {
            heater_id,
            heater_name: heater_name.to_string(),
            success,
            new_pid: PidParams::new(kp, ki, kd),
            ultimate_gain,
            ultimate_period,
            cycles_completed,
            duration_ms: duration_raw,
            error_code,
        })
    }
    
    /// Parse ACK frame payload
    ///
    /// Format: [SUB_TYPE:1][RESULT:1][ERROR_CODE:1]
    ///
    /// # Returns
    /// (sub_type, success, error_code)
    pub fn parse_ack(payload: &[u8]) -> Result<(u8, bool, u8), EmbError> {
        if payload.len() < 4 {
            return Err(EmbError::Protocol("Invalid ACK payload length".to_string()));
        }
        
        let sub_type = payload[1];
        let success = payload[2] != 0;
        let error_code = payload[3];
        
        Ok((sub_type, success, error_code))
    }
    
    /// Convert float to big-endian bytes
    fn float_to_bytes(value: f32) -> [u8; 4] {
        let bytes = value.to_be_bytes();
        bytes
    }
    
    /// Convert big-endian bytes to float
    fn bytes_to_float(bytes: &[u8]) -> f32 {
        if bytes.len() < 4 {
            return 0.0;
        }
        f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_build_start_payload() {
        let payload = PidTuneProtocol::build_start_payload(1, 200.0, 6);
        
        assert_eq!(payload[0], PidTuneSubType::Start as u8);
        assert_eq!(payload[1], 1);           // heater_id
        assert_eq!(payload[2], 0x07);        // temp high byte (2000 = 0x07D0)
        assert_eq!(payload[3], 0xD0);        // temp low byte
        assert_eq!(payload[4], 6);           // cycles
    }
    
    #[test]
    fn test_build_cancel_payload() {
        let payload = PidTuneProtocol::build_cancel_payload(1);
        
        assert_eq!(payload[0], PidTuneSubType::Cancel as u8);
        assert_eq!(payload[1], 1);
    }
    
    #[test]
    fn test_build_apply_payload() {
        let params = PidParams::new(14.5, 0.58, 92.3);
        let payload = PidTuneProtocol::build_apply_payload(1, &params);
        
        assert_eq!(payload[0], PidTuneSubType::Apply as u8);
        assert_eq!(payload[1], 1);
        assert_eq!(payload.len(), 14);  // 2 + 4*3
    }
    
    #[test]
    fn test_parse_progress() {
        // Simulate a progress payload
        let payload = vec![
            PidTuneSubType::Progress as u8,
            1,              // heater_id
            2,              // phase = Measuring
            3,              // current_cycle
            6,              // total_cycles
            0x07, 0xC2,     // current_temp = 1986 (198.6°C)
            100,            // output = 100 (50%)
        ];
        
        let progress = PidTuneProtocol::parse_progress(&payload).unwrap();
        
        assert_eq!(progress.heater_id, 1);
        assert_eq!(progress.phase, TunePhase::Measuring);
        assert_eq!(progress.current_cycle, 3);
        assert_eq!(progress.total_cycles, 6);
        assert!((progress.current_temp - 198.6).abs() < 0.1);
        assert!((progress.output_power - 0.5).abs() < 0.01);
    }
    
    #[test]
    fn test_float_conversion() {
        let value = 14.5f32;
        let bytes = PidTuneProtocol::float_to_bytes(value);
        let recovered = PidTuneProtocol::bytes_to_float(&bytes);
        
        assert!((value - recovered).abs() < 0.001);
    }
}
