//! Authentication Handlers
//!
//! HTTP handlers for authentication endpoints.

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{WebServerState, middleware::auth};

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Access password (simple password protection)
    pub password: String,
}

/// Login response
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// JWT token
    pub token: String,
    /// Token type (always "Bearer")
    pub token_type: String,
    /// Expires in seconds
    pub expires_in: i64,
}

/// Login handler - validate password and generate JWT token
pub async fn login(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    // Check if authentication is enabled
    if !state.config.enable_auth {
        // If auth is disabled, still generate a token for consistency
        let secret = state.config.jwt_secret.as_ref()
            .map(|s| s.as_str())
            .unwrap_or("default-secret");
        
        let expires_in_hours = 24;
        let token = auth::generate_token("user", secret, expires_in_hours)
            .map_err(|e| {
                log::error!("Failed to generate token: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        
        return Ok(Json(LoginResponse {
            token,
            token_type: "Bearer".to_string(),
            expires_in: expires_in_hours * 3600,
        }));
    }
    
    // Get expected password
    let expected_password = state.config.access_password.as_ref()
        .ok_or_else(|| {
            log::error!("Access password not configured");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    // Validate password
    if req.password != *expected_password {
        log::warn!("Invalid password attempt");
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    // Get JWT secret
    let secret = state.config.jwt_secret.as_ref()
        .ok_or_else(|| {
            log::error!("JWT secret not configured");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    // Generate token (expires in 24 hours)
    let expires_in_hours = 24;
    let token = auth::generate_token("user", secret, expires_in_hours)
        .map_err(|e| {
            log::error!("Failed to generate token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    log::info!("User logged in successfully");
    
    Ok(Json(LoginResponse {
        token,
        token_type: "Bearer".to_string(),
        expires_in: expires_in_hours * 3600,
    }))
}

/// Validate token handler - validate JWT token
pub async fn validate_token(
    State(state): State<Arc<WebServerState>>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Get JWT secret
    let secret = state.config.jwt_secret.as_ref()
        .ok_or_else(|| {
            log::error!("JWT secret not configured");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    // Get token from request
    let token = req.get("token")
        .and_then(|t| t.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    // Validate token
    let claims = auth::validate_token(token, secret)
        .map_err(|e| {
            log::warn!("Token validation failed: {}", e);
            StatusCode::UNAUTHORIZED
        })?;
    
    Ok(Json(serde_json::json!({
        "valid": true,
        "user_id": claims.sub,
        "expires_at": claims.exp,
    })))
}
