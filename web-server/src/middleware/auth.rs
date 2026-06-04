//! Authentication Middleware
//!
//! JWT-based authentication middleware for API endpoints.

use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    body::Body,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use chrono::{Duration, Utc};

use crate::WebServerState;

/// JWT Claims
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Expiration time (as UTC timestamp)
    pub exp: usize,
    /// Issued at (as UTC timestamp)
    pub iat: usize,
}

/// JWT authentication middleware
pub async fn auth_middleware(
    State(state): State<Arc<WebServerState>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip authentication if disabled
    if !state.config.enable_auth {
        return Ok(next.run(request).await);
    }
    
    // Get JWT secret
    let secret = state.config.jwt_secret.as_ref()
        .ok_or_else(|| {
            log::error!("JWT secret not configured");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    // Get Authorization header
    let auth_header = request.headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Extract token from "Bearer <token>"
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Validate JWT
    let decoding_key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::new(Algorithm::HS256);
    
    let claims = decode::<Claims>(token, &decoding_key, &validation)
        .map_err(|e| {
            log::warn!("JWT validation failed: {}", e);
            StatusCode::UNAUTHORIZED
        })?;
    
    // Add claims to request extensions
    let mut request = request;
    request.extensions_mut().insert(claims.claims);
    
    Ok(next.run(request).await)
}

/// Generate JWT token
pub fn generate_token(user_id: &str, secret: &str, expires_in_hours: i64) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let exp = now + Duration::hours(expires_in_hours);
    
    let claims = Claims {
        sub: user_id.to_string(),
        exp: exp.timestamp() as usize,
        iat: now.timestamp() as usize,
    };
    
    let encoding_key = EncodingKey::from_secret(secret.as_bytes());
    encode(&Header::default(), &claims, &encoding_key)
}

/// Validate JWT token
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let decoding_key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::new(Algorithm::HS256);
    
    let token_data = decode::<Claims>(token, &decoding_key, &validation)?;
    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate_token() {
        let secret = "test-secret";
        let user_id = "test-user";
        
        let token = generate_token(user_id, secret, 1).unwrap();
        let claims = validate_token(&token, secret).unwrap();
        
        assert_eq!(claims.sub, user_id);
    }
}
