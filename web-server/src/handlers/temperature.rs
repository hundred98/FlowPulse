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

use crate::WebServerState;

/// Temperature status
#[derive(Debug, Serialize, Deserialize)]
pub struct TemperatureStatus {
    pub hotend_current: f32,
    pub hotend_target: f32,
    pub bed_current: f32,
    pub bed_target: f32,
}

/// Get temperature status
pub async fn get_temperature(
    State(_state): State<Arc<WebServerState>>,
) -> Result<Json<TemperatureStatus>, StatusCode> {
    // TODO: Implement actual temperature retrieval
    let temp = TemperatureStatus {
        hotend_current: 25.0,
        hotend_target: 0.0,
        bed_current: 25.0,
        bed_target: 0.0,
    };
    
    Ok(Json(temp))
}
