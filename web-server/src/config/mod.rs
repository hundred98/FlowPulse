//! Web Server Configuration
//!
//! Configuration for the Axum-based web server with memory optimization settings.

use serde::{Deserialize, Serialize};

/// Web server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebServerConfig {
    /// Server bind address
    pub bind_address: String,
    /// Server port
    pub port: u16,
    /// Maximum connections (memory optimization)
    pub max_connections: usize,
    /// Enable CORS
    pub enable_cors: bool,
    /// Enable authentication
    pub enable_auth: bool,
    /// Access password (simple password protection, no username required)
    pub access_password: Option<String>,
    /// JWT secret (optional, auto-generated if not provided)
    pub jwt_secret: Option<String>,
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
    /// Enable static file serving (for development)
    pub serve_static_files: bool,
    /// Static files directory
    pub static_files_dir: Option<String>,
}

impl Default for WebServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
            max_connections: 5, // Limit for embedded Linux
            enable_cors: true,
            enable_auth: false,
            access_password: None,
            jwt_secret: None,
            request_timeout_secs: 30,
            serve_static_files: false,
            static_files_dir: None,
        }
    }
}

impl WebServerConfig {
    /// Create a new configuration with memory-optimized defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Create configuration for development environment
    pub fn development() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            max_connections: 10,
            enable_cors: true,
            enable_auth: false,
            access_password: None,
            jwt_secret: None,
            request_timeout_secs: 60,
            serve_static_files: true,
            static_files_dir: Some("./web/dist".to_string()),
        }
    }

    /// Create configuration for production environment
    pub fn production() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
            max_connections: 5, // Strict limit for embedded Linux
            enable_cors: false,
            enable_auth: false, // Default to no auth for local network
            access_password: None,
            jwt_secret: Some("change-this-secret".to_string()),
            request_timeout_secs: 30,
            serve_static_files: false,
            static_files_dir: None,
        }
    }

    /// Create configuration with simple password protection
    pub fn with_password(password: &str) -> Self {
        Self {
            enable_auth: true,
            access_password: Some(password.to_string()),
            jwt_secret: Some(uuid::Uuid::new_v4().to_string()), // Auto-generate secret
            ..Self::default()
        }
    }

    /// Load configuration from file
    pub fn from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;
        
        let mut config: Self = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config: {}", e))?;
        
        // Auto-generate JWT secret if not provided
        if config.jwt_secret.is_none() {
            config.jwt_secret = Some(uuid::Uuid::new_v4().to_string());
        }
        
        Ok(config)
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let mut config = Self::default();
        
        // BIND_ADDRESS
        if let Ok(addr) = std::env::var("FLOWPULSE_BIND_ADDRESS") {
            config.bind_address = addr;
        }
        
        // PORT
        if let Ok(port) = std::env::var("FLOWPULSE_PORT") {
            if let Ok(p) = port.parse() {
                config.port = p;
            }
        }
        
        // ENABLE_AUTH
        if let Ok(auth) = std::env::var("FLOWPULSE_ENABLE_AUTH") {
            config.enable_auth = auth == "true" || auth == "1";
        }
        
        // ACCESS_PASSWORD
        if let Ok(password) = std::env::var("FLOWPULSE_ACCESS_PASSWORD") {
            config.access_password = Some(password);
            config.enable_auth = true;
        }
        
        // JWT_SECRET
        if let Ok(secret) = std::env::var("FLOWPULSE_JWT_SECRET") {
            config.jwt_secret = Some(secret);
        }
        
        // Auto-generate JWT secret if auth enabled but no secret
        if config.enable_auth && config.jwt_secret.is_none() {
            config.jwt_secret = Some(uuid::Uuid::new_v4().to_string());
        }
        
        config
    }

    /// Load configuration with priority: file > env > default
    pub fn load(config_path: Option<&str>) -> Result<Self, String> {
        // Try to load from file first
        if let Some(path) = config_path {
            if std::path::Path::new(path).exists() {
                log::info!("Loading config from file: {}", path);
                return Self::from_file(path);
            } else {
                log::warn!("Config file not found: {}, using defaults", path);
            }
        }
        
        // Try environment variables
        let has_env = std::env::var("FLOWPULSE_BIND_ADDRESS").is_ok() ||
                      std::env::var("FLOWPULSE_PORT").is_ok() ||
                      std::env::var("FLOWPULSE_ACCESS_PASSWORD").is_ok();
        
        if has_env {
            log::info!("Loading config from environment variables");
            return Ok(Self::from_env());
        }
        
        // Use default
        log::info!("Using default config");
        Ok(Self::default())
    }

    /// Get the server address
    pub fn server_address(&self) -> String {
        format!("{}:{}", self.bind_address, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WebServerConfig::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.max_connections, 5);
        assert!(config.enable_cors);
        assert!(!config.enable_auth);
    }

    #[test]
    fn test_development_config() {
        let config = WebServerConfig::development();
        assert_eq!(config.max_connections, 10);
        assert!(config.serve_static_files);
    }

    #[test]
    fn test_production_config() {
        let config = WebServerConfig::production();
        assert_eq!(config.max_connections, 5);
        assert!(config.enable_auth);
        assert!(!config.serve_static_files);
    }
}
