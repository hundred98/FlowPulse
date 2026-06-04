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
