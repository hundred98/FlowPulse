//! Printer Host V2 — Socket Architecture
//!
//! Replaces direct serial driver usage with CoreSocketClient (TCP Socket to emb-core-server).
//! Provides the same high-level printing interface as PrinterHost but through
//! the socket-based client/server architecture.

use std::sync::Arc;

use emb_public::{CoreSocketClient, CoreClientConfig};
use emb_api::StatusResponse;

/// V2 Host configuration
#[derive(Debug, Clone)]
pub struct HostV2Config {
    #[allow(dead_code)]
    /// Path to printer JSON config file
    pub config_path: String,
    /// Socket server address (default: "127.0.0.1:9527")
    pub server_addr: String,
    /// Connection timeout in milliseconds
    pub connect_timeout_ms: u64,
    /// Default request timeout in milliseconds
    pub request_timeout_ms: u64,
}

impl Default for HostV2Config {
    fn default() -> Self {
        Self {
            config_path: "printer_config.json".to_string(),
            server_addr: "127.0.0.1:9527".to_string(),
            connect_timeout_ms: 5000,
            request_timeout_ms: 10000,
        }
    }
}

/// Printer Host V2 — Socket-based host.
///
/// Uses CoreSocketClient to communicate with emb-core-server over TCP,
/// delegating serial communication and motion planning to the server.
pub struct PrinterHostV2 {
    client: Arc<CoreSocketClient>,
    config: HostV2Config,
}

#[allow(dead_code)]
impl PrinterHostV2 {
    /// Create a new V2 host with custom config.
    pub fn new(config: HostV2Config) -> Self {
        let client_config = CoreClientConfig {
            server_addr: config.server_addr.clone(),
            connect_timeout_ms: config.connect_timeout_ms,
            request_timeout_ms: config.request_timeout_ms,
            auto_reconnect: true,
        };

        let client = CoreSocketClient::new(client_config);

        Self {
            client: Arc::new(client),
            config,
        }
    }

    /// Create with default config.
    pub fn with_defaults(config_path: &str) -> Self {
        Self::new(HostV2Config {
            config_path: config_path.to_string(),
            ..Default::default()
        })
    }

    // ── Connection lifecycle ──────────────────────────────────────

    /// Connect TCP socket to the server (without serial).
    /// Motion planning works immediately after this.
    pub async fn connect_socket(&self) -> Result<(), String> {
        self.client.connect().await
            .map_err(|e| format!("Socket connect failed: {}", e))?;

        log::info!("Connected to emb-core-server at {}", self.config.server_addr);

        self.client.ping().await
            .map_err(|e| format!("Ping failed: {}", e))?;

        log::info!("Server ping OK");
        Ok(())
    }

    /// Connect to the socket server and fully initialize serial.
    pub async fn connect(&self) -> Result<(), String> {
        self.connect_socket().await?;

        // Load printer config to the server
        self.client.config_load(&self.config.config_path).await
            .map_err(|e| format!("Load config failed: {}", e))?;

        log::info!("Printer config loaded from {}", self.config.config_path);

        // Send config complete + init device seq
        self.client.serial_config_complete().await
            .map_err(|e| format!("Config complete failed: {}", e))?;

        self.client.serial_init_seq().await
            .map_err(|e| format!("Init device seq failed: {}", e))?;

        log::info!("Device initialized");
        Ok(())
    }

    /// Close socket without disconnecting serial.
    /// Serial write task continues running until all pending frames are sent.
    pub async fn disconnect(&self) -> Result<(), String> {
        let _ = self.client.disconnect().await;
        log::info!("Disconnected from server (serial kept alive)");
        Ok(())
    }

    // ── Motion operations ─────────────────────────────────────────

    /// Dispatch a linear move (G0/G1) to the server.
    /// Server handles planning → mm→steps → batch → serial send.
    /// Returns the number of segments dispatched.
    pub async fn dispatch_linear_move(
        &self,
        cmd: &str,
        x: Option<f32>, y: Option<f32>, z: Option<f32>, e: Option<f32>,
        feed_rate: Option<f32>,
    ) -> Result<usize, String> {
        self.client.motion_dispatch(cmd, x, y, z, e, feed_rate).await
    }

    /// Dispatch an arc move (G2/G3) to the server.
    pub async fn dispatch_arc_move(
        &self,
        cmd: &str,
        x: Option<f32>, y: Option<f32>, z: Option<f32>, e: Option<f32>,
        feed_rate: Option<f32>,
        i: f32, j: f32,
    ) -> Result<usize, String> {
        self.client.motion_dispatch_arc(cmd, x, y, z, e, feed_rate, Some(emb_api::ArcParamsApi {
            i, j,
            direction: if cmd.to_uppercase().starts_with("G2") { 0 } else { 1 },
        })).await
    }

    /// Home all axes (G28).
    pub async fn home(&self) -> Result<(), String> {
        self.client.motion_reset_position().await
    }

    /// Set current position (G92).
    pub async fn set_position(
        &self, x: Option<f32>, y: Option<f32>, z: Option<f32>, e: Option<f32>,
    ) -> Result<(), String> {
        self.client.motion_set_position(x, y, z, e).await
    }

    /// Get current position.
    pub async fn get_position(&self) -> Result<(f32, f32, f32, f32), String> {
        self.client.motion_get_position().await
    }

    // ── Raw serial operations (passthrough) ────────────────────────

    /// Send raw bytes (pre-built packed frame) through serial.
    pub async fn send_raw_bytes(&self, data: &[u8]) -> Result<(), String> {
        self.client.serial_send_raw(data).await
    }

    /// Send a frame by type + payload.
    pub async fn send_frame(&self, frame_type: u8, payload: Vec<u8>) -> Result<(), String> {
        self.client.serial_send_frame(frame_type, payload).await
    }

    /// Receive next available frame.
    pub async fn recv_frame(&self) -> Result<Option<(u8, Vec<u8>)>, String> {
        self.client.serial_recv_frame().await
    }

    /// Query serial status.
    pub async fn serial_status(&self) -> Result<(bool, Option<String>), String> {
        self.client.serial_query_status().await
    }

    /// Query server status.
    pub async fn server_status(&self) -> Result<StatusResponse, String> {
        self.client.query_status().await
    }

    // ── Accessors ────────────────────────────────────────────────

    /// Get the underlying CoreSocketClient.
    pub fn client(&self) -> &CoreSocketClient {
        &self.client
    }

    /// Get config reference.
    pub fn config(&self) -> &HostV2Config {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_v2_config_default() {
        let config = HostV2Config::default();
        assert_eq!(config.server_addr, "127.0.0.1:9527");
        assert_eq!(config.connect_timeout_ms, 5000);
    }

    #[test]
    fn test_host_v2_with_defaults() {
        let host = PrinterHostV2::with_defaults("test_config.json");
        assert_eq!(host.config().config_path, "test_config.json");
    }
}
