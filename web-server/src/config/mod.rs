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
    /// JWT secret (optional)
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
            enable_auth: true,
            jwt_secret: Some("change-this-secret".to_string()),
            request_timeout_secs: 30,
            serve_static_files: false,
            static_files_dir: None,
        }
    }

    /// Load configuration from file
    pub fn from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;
        
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config: {}", e))
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
