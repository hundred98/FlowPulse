//! Core Socket Client for emb-public
//!
//! Connects to emb-core-server over TCP Socket and provides
//! convenient methods for all CoreRequest/CoreResponse operations.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, RwLock};
use log::{info, warn, debug};

use emb_api::{
    CoreRequest, CoreResponse,
    SerialRequest, SerialResponse,
    MotionRequest, MotionResponse, ArcParamsApi,
    ConfigRequest, ConfigResponse, StatusResponse,
    MotionStatsResponse,
    encode_request, decode_response,
};

/// Client configuration
#[derive(Debug, Clone)]
pub struct CoreClientConfig {
    /// Server TCP address (e.g. "127.0.0.1:9527")
    pub server_addr: String,
    /// Connection timeout (ms)
    pub connect_timeout_ms: u64,
    /// Request timeout (ms)
    pub request_timeout_ms: u64,
    /// Auto-reconnect on send failure
    pub auto_reconnect: bool,
}

impl Default for CoreClientConfig {
    fn default() -> Self {
        Self {
            server_addr: "127.0.0.1:9527".to_string(),
            connect_timeout_ms: 5000,
            request_timeout_ms: 10000,
            auto_reconnect: true,
        }
    }
}

/// Socket client that connects to emb-core-server.
pub struct CoreSocketClient {
    config: CoreClientConfig,
    stream: RwLock<Option<TcpStream>>,
    read_buf: Mutex<Vec<u8>>,
    /// GPIO Report回调（可选），参数: (name, value)
    gpio_report_callback: RwLock<Option<Box<dyn Fn(String, f32) + Send + Sync>>>,
}

impl CoreSocketClient {
    /// Create a new client with the given config.
    pub fn new(config: CoreClientConfig) -> Self {
        Self {
            config,
            stream: RwLock::new(None),
            read_buf: Mutex::new(Vec::new()),
            gpio_report_callback: RwLock::new(None),
        }
    }

    /// Create with default config.
    pub fn default_client(addr: &str) -> Self {
        Self::new(CoreClientConfig {
            server_addr: addr.to_string(),
            ..Default::default()
        })
    }

    /// Connect to the core server.
    pub async fn connect(&self) -> Result<(), String> {
        info!("Connecting to core server at {}", self.config.server_addr);
        let stream = tokio::time::timeout(
            Duration::from_millis(self.config.connect_timeout_ms),
            TcpStream::connect(&self.config.server_addr),
        )
        .await
        .map_err(|e| format!("Connection timeout: {}", e))?
        .map_err(|e| format!("Connection failed: {}", e))?;

        // Set TCP_NODELAY for low latency
        stream.set_nodelay(true).map_err(|e| format!("set_nodelay failed: {}", e))?;

        let mut guard = self.stream.write().await;
        *guard = Some(stream);

        // Clear any stale read buffer
        let mut buf = self.read_buf.lock().await;
        buf.clear();

        info!("Connected to core server");
        Ok(())
    }

    /// Disconnect from the core server.
    pub async fn disconnect(&self) {
        let mut guard = self.stream.write().await;
        if let Some(mut stream) = guard.take() {
            let _ = stream.shutdown().await;
        }
        info!("Disconnected from core server");
    }
    
    /// 设置GPIO Report回调，参数: (name, value)
    pub async fn set_gpio_report_callback<F>(&self, callback: F)
    where
        F: Fn(String, f32) + Send + Sync + 'static,
    {
        let mut guard = self.gpio_report_callback.write().await;
        *guard = Some(Box::new(callback));
    }
    
    /// 清除GPIO Report回调
    pub async fn clear_gpio_report_callback(&self) {
        let mut guard = self.gpio_report_callback.write().await;
        *guard = None;
    }

    /// Check if connected.
    pub async fn is_connected(&self) -> bool {
        let guard = self.stream.read().await;
        guard.is_some()
    }

    /// Ensure connection is active, reconnect if needed.
    async fn ensure_connected(&self) -> Result<(), String> {
        if self.is_connected().await {
            return Ok(());
        }
        if self.config.auto_reconnect {
            warn!("Not connected, attempting auto-reconnect...");
            self.connect().await
        } else {
            Err("Not connected to core server".to_string())
        }
    }

    /// Send a raw request and receive a raw response.
    pub async fn send_request(&self, request: &CoreRequest) -> Result<CoreResponse, String> {
        self.ensure_connected().await?;
        self.send_request_inner(request).await
    }

    /// Internal send without auto-reconnect check.
    async fn send_request_inner(&self, request: &CoreRequest) -> Result<CoreResponse, String> {
        let encoded = encode_request(request)
            .map_err(|e| format!("Encode error: {}", e))?;

        let mut guard = self.stream.write().await;
        let stream = guard.as_mut().ok_or("Not connected")?;

        debug!("Sending request: {:?}", request);
        stream.write_all(&encoded).await
            .map_err(|e| format!("Write error: {}", e))?;
        stream.flush().await
            .map_err(|e| format!("Flush error: {}", e))?;

        // Drop write lock before reading response
        drop(guard);

        self.read_response().await
    }

    /// Read a response from the server.
    async fn read_response(&self) -> Result<CoreResponse, String> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(self.config.request_timeout_ms);

        loop {
            // Check if we have a complete message in buffer
            {
                let mut buf = self.read_buf.lock().await;
                if !buf.is_empty() {
                    // Decode without holding mutable borrow
                    let buf_snapshot: Vec<u8> = buf.clone();
                    match decode_response(&buf_snapshot) {
                        Ok((remaining, response)) => {
                            // Consume the decoded portion
                            let consumed = buf_snapshot.len() - remaining.len();
                            buf.drain(..consumed);
                            
                            // 检查是否是GPIO Report推送
                            if let CoreResponse::Gpio(emb_api::GpioResponse::PinReport { name, value }) = &response {
                                // 触发回调
                                let callback_guard = self.gpio_report_callback.read().await;
                                if let Some(callback) = callback_guard.as_ref() {
                                    callback(name.clone(), *value);
                                }
                                // 继续读取下一个消息
                                continue;
                            }
                            
                            return Ok(response);
                        }
                        Err(e) => {
                            if !e.contains("too short") {
                                buf.clear();
                                return Err(format!("Decode error: {}", e));
                            }
                        }
                    }
                }
            }

            // Check timeout
            if tokio::time::Instant::now() > deadline {
                return Err("Request timeout".to_string());
            }

            // Read more data
            let mut guard = self.stream.write().await;
            let stream = guard.as_mut().ok_or("Not connected")?;
            let mut tmp = [0u8; 8192];
            match stream.read(&mut tmp).await {
                Ok(0) => return Err("Server closed connection".to_string()),
                Ok(n) => {
                    let mut buf = self.read_buf.lock().await;
                    buf.extend_from_slice(&tmp[..n]);
                }
                Err(e) => return Err(format!("Read error: {}", e)),
            }
        }
    }

    // ========================================================================
    // Ping
    // ========================================================================

    /// Ping the server.
    pub async fn ping(&self) -> Result<(), String> {
        match self.send_request(&CoreRequest::Ping).await? {
            CoreResponse::Pong => Ok(()),
            CoreResponse::Error(e) => Err(format!("Ping error: {}", e.message)),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    // ========================================================================
    // Status
    // ========================================================================

    /// Query server status.
    pub async fn query_status(&self) -> Result<StatusResponse, String> {
        match self.send_request(&CoreRequest::QueryStatus).await? {
            CoreResponse::Status(s) => Ok(s),
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    // ========================================================================
    // Serial operations
    // ========================================================================

    /// Connect to a serial port.
    pub async fn serial_connect(&self, port: &str, baud_rate: u32) -> Result<(), String> {
        match self.send_request(&CoreRequest::Serial(SerialRequest::Connect {
            port: port.to_string(),
            baud_rate,
        })).await? {
            CoreResponse::Serial(SerialResponse::ConnectResult { success: true, .. }) => Ok(()),
            CoreResponse::Serial(SerialResponse::ConnectResult { success: false, error }) => {
                Err(error.unwrap_or_else(|| "Connect failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Disconnect serial port.
    pub async fn serial_disconnect(&self) -> Result<(), String> {
        match self.send_request(&CoreRequest::Serial(SerialRequest::Disconnect)).await? {
            CoreResponse::Serial(SerialResponse::DisconnectResult { success: true, .. }) => Ok(()),
            CoreResponse::Serial(SerialResponse::DisconnectResult { success: false, error }) => {
                Err(error.unwrap_or_else(|| "Disconnect failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Send a raw packed frame to the device.
    pub async fn serial_send_raw(&self, data: &[u8]) -> Result<(), String> {
        match self.send_request(&CoreRequest::Serial(SerialRequest::SendRawBytes {
            data: data.to_vec(),
        })).await? {
            CoreResponse::Serial(SerialResponse::FrameSendResult { success: true, .. }) => Ok(()),
            CoreResponse::Serial(SerialResponse::FrameSendResult { success: false, error }) => {
                Err(error.unwrap_or_else(|| "Send failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Send a frame by type + payload.
    pub async fn serial_send_frame(&self, frame_type: u8, payload: Vec<u8>) -> Result<(), String> {
        match self.send_request(&CoreRequest::Serial(SerialRequest::SendFrame {
            frame_type, payload,
        })).await? {
            CoreResponse::Serial(SerialResponse::FrameSendResult { success: true, .. }) => Ok(()),
            CoreResponse::Serial(SerialResponse::FrameSendResult { success: false, error }) => {
                Err(error.unwrap_or_else(|| "Send failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Receive next available frame (non-blocking).
    pub async fn serial_recv_frame(&self) -> Result<Option<(u8, Vec<u8>)>, String> {
        match self.send_request(&CoreRequest::Serial(SerialRequest::RecvFrame)).await? {
            CoreResponse::Serial(SerialResponse::FrameReceived { frame_type, payload }) => {
                Ok(Some((frame_type, payload)))
            }
            CoreResponse::Serial(SerialResponse::NoFrame) => Ok(None),
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Query serial status.
    pub async fn serial_query_status(&self) -> Result<(bool, Option<String>), String> {
        match self.send_request(&CoreRequest::Serial(SerialRequest::QueryStatus)).await? {
            CoreResponse::Serial(SerialResponse::SerialStatusInfo { connected, port }) => {
                Ok((connected, port))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Send ConfigComplete marker.
    pub async fn serial_config_complete(&self) -> Result<(), String> {
        match self.send_request(&CoreRequest::Serial(SerialRequest::ConfigComplete)).await? {
            CoreResponse::Serial(SerialResponse::AckReceived) => Ok(()),
            CoreResponse::Serial(SerialResponse::NackReceived { error_code }) => {
                Err(format!("NACK received (error_code=0x{:02X})", error_code))
            }
            CoreResponse::Serial(SerialResponse::ConfigResult { success: true, .. }) => Ok(()),
            CoreResponse::Serial(SerialResponse::ConfigResult { success: false, error }) => {
                Err(error.unwrap_or_else(|| "ConfigComplete failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Initialize device sequence number.
    pub async fn serial_init_seq(&self) -> Result<(), String> {
        match self.send_request(&CoreRequest::Serial(SerialRequest::InitDeviceSeq)).await? {
            CoreResponse::Serial(SerialResponse::SeqInitResult) => Ok(()),
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// 进入打印模式 - 打印开始时调用，启用 StatusReport 和运动执行
    pub async fn serial_enter_print_mode(&self) -> Result<(), String> {
        match self.send_request(&CoreRequest::Serial(SerialRequest::EnterPrintMode)).await? {
            CoreResponse::Serial(SerialResponse::ConfigResult { success: true, .. }) => Ok(()),
            CoreResponse::Serial(SerialResponse::ConfigResult { success: false, error }) => {
                Err(error.unwrap_or_else(|| "EnterPrintMode failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// 退出打印模式 - 打印结束时调用
    pub async fn serial_exit_print_mode(&self) -> Result<(), String> {
        match self.send_request(&CoreRequest::Serial(SerialRequest::ExitPrintMode)).await? {
            CoreResponse::Serial(SerialResponse::ConfigResult { success: true, .. }) => Ok(()),
            CoreResponse::Serial(SerialResponse::ConfigResult { success: false, error }) => {
                Err(error.unwrap_or_else(|| "ExitPrintMode failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    // ========================================================================
    // Motion operations
    // ========================================================================

    /// Reset position (G28 home).
    pub async fn motion_reset_position(&self) -> Result<(), String> {
        match self.send_request(&CoreRequest::Motion(MotionRequest::ResetPosition)).await? {
            CoreResponse::Motion(MotionResponse::Acknowledged) => Ok(()),
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Set current position (G92).
    pub async fn motion_set_position(
        &self,
        x: Option<f32>, y: Option<f32>, z: Option<f32>, e: Option<f32>,
    ) -> Result<(), String> {
        match self.send_request(&CoreRequest::Motion(MotionRequest::SetPosition { x, y, z, e })).await? {
            CoreResponse::Motion(MotionResponse::Acknowledged) => Ok(()),
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Get current position.
    pub async fn motion_get_position(&self) -> Result<(f32, f32, f32, f32), String> {
        match self.send_request(&CoreRequest::Motion(MotionRequest::GetPosition)).await? {
            CoreResponse::Motion(MotionResponse::PositionResult { x, y, z, e }) => {
                Ok((x, y, z, e))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Wait for all pending motion frames to be sent and device buffer to drain.
    pub async fn motion_wait_drain(&self) -> Result<(), String> {
        match self.send_request(&CoreRequest::Motion(MotionRequest::WaitMotionDrain)).await? {
            CoreResponse::Motion(MotionResponse::DrainResult { success: true, .. }) => Ok(()),
            CoreResponse::Motion(MotionResponse::DrainResult { success: false, error }) => {
                Err(error.unwrap_or_else(|| "Drain failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Query motion and serial statistics.
    pub async fn motion_query_stats(&self) -> Result<MotionStatsResponse, String> {
        match self.send_request(&CoreRequest::Motion(MotionRequest::QueryStats)).await? {
            CoreResponse::Motion(MotionResponse::Stats(stats)) => Ok(stats),
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Enable or disable motors.
    pub async fn motor_enable(&self, enable_mask: u8) -> Result<(), String> {
        match self.send_request(&CoreRequest::Motion(MotionRequest::MotorEnable { enable_mask })).await? {
            CoreResponse::Motion(MotionResponse::MotorEnableResult { success: true, .. }) => Ok(()),
            CoreResponse::Motion(MotionResponse::MotorEnableResult { success: false, error }) => {
                Err(error.unwrap_or_else(|| "Motor enable failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Plan a move AND dispatch segments to the serial device.
    /// Server handles planning → mm→steps → batch → serial send.
    /// Returns the number of segments dispatched.
    pub async fn motion_dispatch(
        &self,
        cmd: &str,
        x: Option<f32>,
        y: Option<f32>,
        z: Option<f32>,
        e: Option<f32>,
        feed_rate: Option<f32>,
    ) -> Result<usize, String> {
        self.motion_dispatch_arc(cmd, x, y, z, e, feed_rate, None).await
    }

    /// Plan a move (with optional arc) AND dispatch segments to serial.
    pub async fn motion_dispatch_arc(
        &self,
        cmd: &str,
        x: Option<f32>,
        y: Option<f32>,
        z: Option<f32>,
        e: Option<f32>,
        feed_rate: Option<f32>,
        arc: Option<ArcParamsApi>,
    ) -> Result<usize, String> {
        match self.send_request(&CoreRequest::Motion(MotionRequest::DispatchMotion {
            cmd: cmd.to_string(),
            x, y, z, e, feed_rate, arc,
        })).await? {
            CoreResponse::Motion(MotionResponse::DispatchResult { success: true, segments_dispatched, .. }) => {
                Ok(segments_dispatched)
            }
            CoreResponse::Motion(MotionResponse::DispatchResult { success: false, error, .. }) => {
                Err(error.unwrap_or_else(|| "Dispatch failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    // ========================================================================
    // Config operations
    // ========================================================================

    /// Load printer config from file path on the server.
    pub async fn config_load(&self, config_path: &str) -> Result<(), String> {
        match self.send_request(&CoreRequest::Config(ConfigRequest::LoadPrinterConfig {
            config_path: config_path.to_string(),
        })).await? {
            CoreResponse::Config(ConfigResponse::ConfigLoaded { success: true, .. }) => Ok(()),
            CoreResponse::Config(ConfigResponse::ConfigLoaded { success: false, error }) => {
                Err(error.unwrap_or_else(|| "Config load failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Update motion config directly via JSON string.
    pub async fn config_update_motion(&self, motion_config_json: &str) -> Result<(), String> {
        match self.send_request(&CoreRequest::Config(ConfigRequest::UpdateMotionConfig {
            motion_config_json: motion_config_json.to_string(),
        })).await? {
            CoreResponse::Config(ConfigResponse::MotionConfigUpdated { success: true, .. }) => Ok(()),
            CoreResponse::Config(ConfigResponse::MotionConfigUpdated { success: false, error }) => {
                Err(error.unwrap_or_else(|| "Config update failed".to_string()))
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Load all config files from a directory (printer.json, motion.json, hardware.json).
    pub async fn load_all_configs(&self, config_dir: &str) -> Result<(bool, bool, bool), String> {
        match self.send_request(&CoreRequest::Config(ConfigRequest::LoadAllConfigs {
            config_dir: config_dir.to_string(),
        })).await? {
            CoreResponse::Config(ConfigResponse::AllConfigsLoaded {
                success: true,
                printer_loaded,
                motion_loaded,
                hardware_loaded,
                ..
            }) => Ok((printer_loaded, motion_loaded, hardware_loaded)),
            CoreResponse::Config(ConfigResponse::AllConfigsLoaded {
                success: false,
                error,
                ..
            }) => Err(error.unwrap_or_else(|| "Load all configs failed".to_string())),
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    // ========================================================================
    // GPIO operations
    // ========================================================================

    /// Set GPIO pin value.
    pub async fn gpio_set(&self, name: &str, value: f32) -> Result<bool, String> {
        match self.send_request(&CoreRequest::Gpio(emb_api::GpioRequest::SetPin {
            name: name.to_string(),
            value,
        })).await? {
            CoreResponse::Gpio(emb_api::GpioResponse::SetPinResult { success, error }) => {
                if success {
                    Ok(true)
                } else {
                    Err(error.unwrap_or_else(|| "Set pin failed".to_string()))
                }
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }

    /// Query GPIO pin value.
    pub async fn gpio_query(&self, name: &str) -> Result<f32, String> {
        match self.send_request(&CoreRequest::Gpio(emb_api::GpioRequest::QueryPin {
            name: name.to_string(),
        })).await? {
            CoreResponse::Gpio(emb_api::GpioResponse::QueryPinResult { value, .. }) => Ok(value),
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }
    
    /// 订阅GPIO Report事件
    pub async fn gpio_subscribe_report(&self, enable: bool) -> Result<(), String> {
        match self.send_request(&CoreRequest::Gpio(emb_api::GpioRequest::SubscribeReport { enable })).await? {
            CoreResponse::Gpio(emb_api::GpioResponse::SubscribeResult { success, error }) => {
                if success {
                    Ok(())
                } else {
                    Err(error.unwrap_or_else(|| "Subscribe failed".to_string()))
                }
            }
            CoreResponse::Error(e) => Err(e.message),
            other => Err(format!("Unexpected response: {:?}", other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_connect_ping() {
        // Start a mock echo server that responds to Pong
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            loop {
                if let Ok((stream, _)) = listener.accept().await {
                    tokio::spawn(async move {
                        // Handle one request then close
                        let mut stream = stream;
                        let mut buf = [0u8; 4096];
                        let n = stream.read(&mut buf).await.unwrap_or(0);
                        if n > 4 {
                            // Parse request, respond with Pong
                            match decode_request(&buf[..n]) {
                                Ok((_, _req)) => {
                                    let resp = CoreResponse::Pong;
                                    let encoded = encode_response(&resp).unwrap();
                                    let _ = stream.write_all(&encoded).await;
                                }
                                Err(_) => {}
                            }
                        }
                    });
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let client = CoreSocketClient::default_client(&addr.to_string());
        client.connect().await.unwrap();
        assert!(client.is_connected().await);

        client.ping().await.unwrap();
        client.disconnect().await;
        assert!(!client.is_connected().await);
    }

    use emb_api::decode_request;
}
