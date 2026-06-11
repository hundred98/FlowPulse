//! Temperature Control Handlers
//!
//! HTTP handlers for temperature control endpoints.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use log::info;

use crate::WebServerState;
use emb_public::temperature::pid_tune::PidTuneProtocol;

/// Temperature status
#[derive(Debug, Serialize, Deserialize)]
pub struct TemperatureStatus {
    pub hotend_current: f32,
    pub hotend_target: f32,
    pub bed_current: f32,
    pub bed_target: f32,
}

/// Temperature target request
#[derive(Debug, Deserialize)]
pub struct SetTemperatureRequest {
    pub hotend: Option<f32>,
    pub bed: Option<f32>,
}

/// API response
#[derive(Debug, Serialize)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
}

/// PID tune start request
#[derive(Debug, Deserialize)]
pub struct PidTuneStartRequest {
    /// Heater to tune: "hotend" or "bed"
    pub heater: String,
    /// Target temperature in °C
    pub target_temp: f32,
    /// Number of tuning cycles (recommended: 6-8)
    pub cycles: Option<u8>,
}

/// PID tune cancel request
#[derive(Debug, Deserialize)]
pub struct PidTuneCancelRequest {
    /// Heater to cancel: "hotend" or "bed"
    pub heater: String,
}

/// PID tune apply request
#[derive(Debug, Deserialize)]
pub struct PidTuneApplyRequest {
    /// Heater to apply: "hotend" or "bed"
    pub heater: String,
    /// PID parameters to apply
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
}

/// PID tune status response
#[derive(Debug, Serialize)]
pub struct PidTuneStatusResponse {
    pub success: bool,
    pub message: String,
    pub heater: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kp: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ki: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kd: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<u8>,
}

/// PID tune progress response
#[derive(Debug, Serialize)]
pub struct PidTuneProgressResponse {
    pub success: bool,
    pub in_progress: bool,
    pub heater_id: u8,
    pub heater: String,
    pub phase: u8,
    pub current_cycle: u8,
    pub total_cycles: u8,
    pub current_temp: f32,
    pub output_power: f32,
    pub message: String,
}

/// Get heater ID from heater name
fn get_heater_id(heater: &str) -> Result<u8, StatusCode> {
    match heater.to_lowercase().as_str() {
        "bed" | "hotbed" => Ok(0),
        "hotend" | "nozzle" => Ok(1),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

/// Get temperature status
pub async fn get_temperature(
    State(state): State<Arc<WebServerState>>,
) -> Result<Json<TemperatureStatus>, StatusCode> {
    // Use FrontendDataProvider to get temperature
    let temp = state.data_provider.get_temperature();
    
    let response = TemperatureStatus {
        hotend_current: temp.hotend_current,
        hotend_target: temp.hotend_target,
        bed_current: temp.bed_current,
        bed_target: temp.bed_target,
    };
    
    Ok(Json(response))
}

/// Set temperature target
pub async fn set_temperature(
    State(state): State<Arc<WebServerState>>,
    Json(request): Json<SetTemperatureRequest>,
) -> Result<Json<ApiResponse>, StatusCode> {
    // Set hotend temperature
    if let Some(hotend_temp) = request.hotend {
        match state.temperature_manager.set_target("hotend", hotend_temp).await {
            Ok(()) => info!("Set hotend temperature to {}°C", hotend_temp),
            Err(e) => {
                log::error!("Failed to set hotend temperature: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }
    
    // Set bed temperature
    if let Some(bed_temp) = request.bed {
        match state.temperature_manager.set_target("bed", bed_temp).await {
            Ok(()) => info!("Set bed temperature to {}°C", bed_temp),
            Err(e) => {
                log::error!("Failed to set bed temperature: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }
    
    Ok(Json(ApiResponse {
        success: true,
        message: "Temperature target set successfully".to_string(),
    }))
}

/// Start PID auto-tune
pub async fn pid_tune_start(
    State(state): State<Arc<WebServerState>>,
    Json(request): Json<PidTuneStartRequest>,
) -> Result<Json<PidTuneStatusResponse>, StatusCode> {
    // Default cycles to 6 if not specified
    let cycles = request.cycles.unwrap_or(6);
    
    info!("Starting PID tune for {}: target={}°C, cycles={}", 
        request.heater, request.target_temp, cycles);
    
    // Use TemperatureManager's start_pid_tune method
    match state.temperature_manager.start_pid_tune(&request.heater, request.target_temp, cycles).await {
        Ok(()) => {
            info!("PID tune started successfully");
            Ok(Json(PidTuneStatusResponse {
                success: true,
                message: format!("PID tune started for {} at {}°C", request.heater, request.target_temp),
                heater: request.heater,
                kp: None,
                ki: None,
                kd: None,
                error_code: None,
            }))
        }
        Err(e) => {
            log::error!("Failed to start PID tune: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Cancel PID auto-tune
pub async fn pid_tune_cancel(
    State(state): State<Arc<WebServerState>>,
    Json(request): Json<PidTuneCancelRequest>,
) -> Result<Json<PidTuneStatusResponse>, StatusCode> {
    info!("Canceling PID tune for {}", request.heater);
    
    // Use TemperatureManager's cancel_pid_tune method
    match state.temperature_manager.cancel_pid_tune().await {
        Ok(()) => {
            info!("PID tune canceled successfully");
            Ok(Json(PidTuneStatusResponse {
                success: true,
                message: format!("PID tune canceled for {}", request.heater),
                heater: request.heater,
                kp: None,
                ki: None,
                kd: None,
                error_code: None,
            }))
        }
        Err(e) => {
            log::error!("Failed to cancel PID tune: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get PID tune progress
pub async fn pid_tune_progress(
    State(state): State<Arc<WebServerState>>,
) -> Result<Json<PidTuneProgressResponse>, StatusCode> {
    // Get current tuning progress
    let progress = state.temperature_manager.get_tune_progress().await;
    let is_tuning = state.temperature_manager.is_tuning().await;
    
    // Debug log
    if let Some(ref p) = progress {
        log::info!("🔄 Progress API: cycle={}, temp={}, power={}, is_tuning={}", 
            p.current_cycle, p.current_temp, p.output_power, is_tuning);
    }
    
    match progress {
        Some(p) => {
            Ok(Json(PidTuneProgressResponse {
                success: true,
                in_progress: is_tuning,
                heater_id: p.heater_id,
                heater: if p.heater_id == 0 { "bed" } else { "hotend" }.to_string(),
                phase: p.phase as u8,
                current_cycle: p.current_cycle,
                total_cycles: p.total_cycles,
                current_temp: p.current_temp,
                output_power: p.output_power,
                message: format!("Tuning: cycle {} of {}", p.current_cycle, p.total_cycles),
            }))
        }
        None => {
            Ok(Json(PidTuneProgressResponse {
                success: true,
                in_progress: is_tuning,
                heater_id: 0,
                heater: "".to_string(),
                phase: 0,
                current_cycle: 0,
                total_cycles: 0,
                current_temp: 0.0,
                output_power: 0.0,
                message: if is_tuning { "Tuning started, waiting for progress" } else { "No tuning in progress" }.to_string(),
            }))
        }
    }
}

/// Apply PID tune result
pub async fn pid_tune_apply(
    State(state): State<Arc<WebServerState>>,
    Json(request): Json<PidTuneApplyRequest>,
) -> Result<Json<PidTuneStatusResponse>, StatusCode> {
    info!("Applying PID tune result for {}: Kp={}, Ki={}, Kd={}", 
        request.heater, request.kp, request.ki, request.kd);
    
    // Get heater ID
    let heater_id = get_heater_id(&request.heater)?;
    
    // Build APPLY frame payload
    use emb_public::temperature::pid_tune::PidParams;
    let params = PidParams::new(request.kp, request.ki, request.kd);
    let payload = PidTuneProtocol::build_apply_payload(heater_id, &params);
    
    // Send to device via core client (TYPE=0x02 for TEMPERATURE)
    match state.temperature_manager.client().send_temperature_tune_frame(&payload).await {
        Ok(()) => {
            info!("PID tune APPLY frame sent successfully");
            Ok(Json(PidTuneStatusResponse {
                success: true,
                message: format!("PID parameters applied for {}", request.heater),
                heater: request.heater,
                kp: Some(request.kp),
                ki: Some(request.ki),
                kd: Some(request.kd),
                error_code: None,
            }))
        }
        Err(e) => {
            log::error!("Failed to send PID tune APPLY frame: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get PID tune result
pub async fn pid_tune_result(
    State(state): State<Arc<WebServerState>>,
) -> Result<Json<PidTuneStatusResponse>, StatusCode> {
    info!("Getting PID tune result");
    
    // Get result from temperature manager
    match state.temperature_manager.get_tune_result().await {
        Some(result) => {
            let heater = if result.heater_id == 0 { "bed" } else { "hotend" };
            info!("PID tune result: heater={}, success={}, Kp={:.3}, Ki={:.3}, Kd={:.3}", 
                heater, result.success, result.new_pid.kp, result.new_pid.ki, result.new_pid.kd);
            
            Ok(Json(PidTuneStatusResponse {
                success: result.success,
                message: if result.success {
                    format!("PID tune complete: Kp={:.3}, Ki={:.3}, Kd={:.3}", 
                        result.new_pid.kp, result.new_pid.ki, result.new_pid.kd)
                } else {
                    format!("PID tune failed: error code {}", result.error_code)
                },
                heater: heater.to_string(),
                kp: Some(result.new_pid.kp),
                ki: Some(result.new_pid.ki),
                kd: Some(result.new_pid.kd),
                error_code: if result.success { None } else { Some(result.error_code) },
            }))
        }
        None => {
            Ok(Json(PidTuneStatusResponse {
                success: false,
                message: "No PID tune result available".to_string(),
                heater: "".to_string(),
                kp: None,
                ki: None,
                kd: None,
                error_code: None,
            }))
        }
    }
}
