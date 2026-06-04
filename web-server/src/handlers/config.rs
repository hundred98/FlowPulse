//! Configuration Handlers
//!
//! HTTP handlers for configuration management endpoints.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde_json::Value;
use std::sync::Arc;

use crate::WebServerState;

/// Get configuration
pub async fn get_config(
    State(_state): State<Arc<WebServerState>>,
) -> Result<Json<Value>, StatusCode> {
    // TODO: Implement actual config retrieval
    let config = serde_json::json!({
        "version": "1.0",
        "printer_model": "FlowPulse",
    });
    
    Ok(Json(config))
}
