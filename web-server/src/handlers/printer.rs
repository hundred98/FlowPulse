//! Printer Control Handlers
//!
//! HTTP handlers for printer control endpoints.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::WebServerState;

/// Printer status response
#[derive(Debug, Serialize, Deserialize)]
pub struct PrinterStatusResponse {
    pub state: String,
    pub position: PositionData,
    pub progress: Option<ProgressData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PositionData {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub e: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProgressData {
    pub percent: f32,
    pub current_layer: u32,
    pub total_layers: u32,
}

/// Generic API response
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

/// Get printer status
pub async fn get_status(
    State(state): State<Arc<WebServerState>>,
) -> Result<Json<PrinterStatusResponse>, StatusCode> {
    // Use FrontendDataProvider to get status
    let printer_status = state.data_provider.get_printer_status();
    let position = state.data_provider.get_position();
    
    let response = PrinterStatusResponse {
        state: printer_status.state,
        position: PositionData {
            x: position.x,
            y: position.y,
            z: position.z,
            e: position.e,
        },
        progress: None,
    };
    
    Ok(Json(response))
}

/// Start print
pub async fn start_print(
    State(_state): State<Arc<WebServerState>>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // TODO: Implement actual print start logic
    Ok(Json(ApiResponse {
        success: true,
        data: Some("Print started".to_string()),
        message: None,
    }))
}

/// Pause print
pub async fn pause_print(
    State(_state): State<Arc<WebServerState>>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // TODO: Implement actual pause logic
    Ok(Json(ApiResponse {
        success: true,
        data: Some("Print paused".to_string()),
        message: None,
    }))
}

/// Resume print
pub async fn resume_print(
    State(_state): State<Arc<WebServerState>>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // TODO: Implement actual resume logic
    Ok(Json(ApiResponse {
        success: true,
        data: Some("Print resumed".to_string()),
        message: None,
    }))
}

/// Stop print
pub async fn stop_print(
    State(_state): State<Arc<WebServerState>>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // TODO: Implement actual stop logic
    Ok(Json(ApiResponse {
        success: true,
        data: Some("Print stopped".to_string()),
        message: None,
    }))
}
