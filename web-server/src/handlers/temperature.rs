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
