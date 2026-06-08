//! emb-api - Shared API types for Socket communication
//!
//! This crate defines the request/response types used between
//! emb-public (client) and emb-core (server) over Socket.
//!
//! Both crates depend on this, so they share the same type definitions
//! without any direct source dependency between them.

use serde::{Deserialize, Serialize};

// ============================================================================
// Core Request (client → server)
// ============================================================================

/// Request message sent from emb-public to emb-core
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CoreRequest {
    /// Health check / keep-alive
    Ping,

    /// Serial port management
    Serial(SerialRequest),

    /// Motion planning
    Motion(MotionRequest),

    /// Configuration management
    Config(ConfigRequest),

    /// GPIO management
    Gpio(GpioRequest),

    /// Query server status
    QueryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GpioRequest {
    SetPin {
        name: String,
        value: f32,
    },
    QueryPin {
        name: String,
    },
    /// 订阅GPIO Report事件
    SubscribeReport {
        /// 是否启用订阅
        enable: bool,
    },
}

/// Serial port related requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerialRequest {
    /// Connect to a serial port
    Connect {
        port: String,
        baud_rate: u32,
    },
    /// Send a raw packed frame (SOF+len+type+payload+CRC+EOF) directly.
    /// Use this for config frames built by the client.
    SendRawBytes {
        data: Vec<u8>,
    },
    /// Send a frame by type + payload
    SendFrame {
        frame_type: u8,
        payload: Vec<u8>,
    },
    /// Receive next available frame (non-blocking)
    RecvFrame,
    /// Query serial connection status
    QueryStatus,
    /// Send ConfigComplete marker frame
    ConfigComplete,
    /// Initialize device sequence number (send reset, wait for ACK)
    InitDeviceSeq,
    /// 进入打印模式 - 打印开始时调用，启用 StatusReport 和运动执行
    EnterPrintMode,
    /// 退出打印模式 - 打印结束时调用
    ExitPrintMode,
    /// 订阅状态上报（DeviceStatusReport）
    SubscribeStatus {
        /// 是否启用订阅
        enable: bool,
    },
}

/// Motion planning related requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MotionRequest {
    /// Plan a move AND dispatch segments to serial device.
    /// Server handles: planning → mm→steps → batch → serial send.
    /// Returns after segments are dispatched (not after device finishes).
    DispatchMotion {
        /// G-code command type (G0/G1/G2/G3/G28/G92, etc.)
        cmd: String,
        /// Target X position (mm), None if not specified
        x: Option<f32>,
        /// Target Y position (mm), None if not specified
        y: Option<f32>,
        /// Target Z position (mm), None if not specified
        z: Option<f32>,
        /// Target E position (mm), None if not specified
        e: Option<f32>,
        /// Feed rate (mm/min), None if not specified
        feed_rate: Option<f32>,
        /// Arc parameters (for G2/G3)
        arc: Option<ArcParamsApi>,
    },
    /// 执行M指令（同步执行）
    /// 前面的运动会规划到速度0停止，然后执行M指令
    ExecuteMCommand {
        /// M指令
        command: MCommand,
    },
    /// Reset position to origin
    ResetPosition,
    /// Set current position (G92)
    SetPosition {
        x: Option<f32>,
        y: Option<f32>,
        z: Option<f32>,
        e: Option<f32>,
    },
    /// Query current position
    GetPosition,
    /// Wait for all pending motion frames to be sent and device buffer to drain.
    /// Blocks until buf_time drops to 0 (device finished executing).
    WaitMotionDrain,
    /// Query motion and serial statistics
    QueryStats,
    /// Enable or disable motors
    MotorEnable {
        /// Motor enable mask (bit0=X, bit1=Y, bit2=Z, bit3=E, 1=enable, 0=disable)
        enable_mask: u8,
    },
}

/// Arc parameters for G2/G3 commands (API-safe subset)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ArcParamsApi {
    /// Arc center offset X (mm)
    pub i: f32,
    /// Arc center offset Y (mm)
    pub j: f32,
    /// Arc direction (0=CW/G2, 1=CCW/G3)
    pub direction: u8,
}

/// M指令执行类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MExecutionType {
    /// 同步等待型 - 需要等待条件满足（如温度达到）
    SyncWait,
    /// 同步设置型 - 执行后立即继续
    SyncSet,
    /// 运动参数型 - 更新运动参数后继续
    MotionParam,
}

/// M指令定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCommand {
    // === 同步等待型 ===
    /// M109 - 设置热端温度并等待
    WaitHotendTemp {
        /// 工具编号
        tool: u8,
        /// 目标温度 (°C)
        temp: f32,
    },
    /// M190 - 设置热床温度并等待
    WaitBedTemp {
        /// 目标温度 (°C)
        temp: f32,
    },

    // === 同步设置型 ===
    /// M104 - 设置热端温度
    SetHotendTemp {
        /// 工具编号
        tool: u8,
        /// 目标温度 (°C)
        temp: f32,
    },
    /// M140 - 设置热床温度
    SetBedTemp {
        /// 目标温度 (°C)
        temp: f32,
    },
    /// M106 - 设置风扇速度
    SetFanSpeed {
        /// 风扇索引
        index: u8,
        /// 速度 (0-255)
        speed: u8,
    },
    /// M107 - 关闭风扇
    FanOff {
        /// 风扇索引
        index: u8,
    },
    /// M82 - 挤出机绝对模式
    ExtruderAbsoluteMode,
    /// M83 - 挤出机相对模式
    ExtruderRelativeMode,

    // === 运动参数型 ===
    /// M201 - 设置加速度
    SetAcceleration {
        x: Option<f32>,
        y: Option<f32>,
        z: Option<f32>,
        e: Option<f32>,
    },
    /// M203 - 设置最大速度
    SetMaxVelocity {
        x: Option<f32>,
        y: Option<f32>,
        z: Option<f32>,
        e: Option<f32>,
    },
    /// M204 - 设置加速度参数
    SetAccelParams {
        /// 移动加速度
        travel: Option<f32>,
        /// 打印加速度
        print: Option<f32>,
        /// 回抽加速度
        retract: Option<f32>,
    },
    /// M92 - 设置步进电机每毫米步数
    SetStepsPerMm {
        x: Option<f32>,
        y: Option<f32>,
        z: Option<f32>,
        e: Option<f32>,
    },
}

impl MCommand {
    /// 获取执行类型
    pub fn execution_type(&self) -> MExecutionType {
        match self {
            MCommand::WaitHotendTemp { .. } | MCommand::WaitBedTemp { .. } => {
                MExecutionType::SyncWait
            }
            MCommand::SetAcceleration { .. }
            | MCommand::SetMaxVelocity { .. }
            | MCommand::SetAccelParams { .. }
            | MCommand::SetStepsPerMm { .. } => MExecutionType::MotionParam,
            _ => MExecutionType::SyncSet,
        }
    }
}

/// Configuration related requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigRequest {
    /// Load and apply a printer configuration
    LoadPrinterConfig {
        /// Path to the JSON config file
        config_path: String,
    },
    /// Update motion config parameters
    UpdateMotionConfig {
        /// Motion config JSON
        motion_config_json: String,
    },
    /// Update fan config (index to GPIO name mapping)
    UpdateFanConfig {
        /// Fan config JSON
        fan_config_json: String,
    },
    /// Load all configuration files from a directory
    LoadAllConfigs {
        /// Path to the config directory containing printer.json, motion.json, hardware.json
        config_dir: String,
    },
}

/// Fan configuration entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanConfigEntry {
    /// Fan index (used in M106/M107 P parameter)
    pub index: u8,
    /// GPIO name (must match a GPIO output definition)
    pub name: String,
    /// Optional description
    #[serde(default)]
    pub description: String,
}

/// Fan configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FanConfig {
    /// Fan entries
    pub fans: Vec<FanConfigEntry>,
}

impl FanConfig {
    /// Get GPIO name for a fan index
    pub fn get_name(&self, index: u8) -> Option<&str> {
        self.fans.iter()
            .find(|f| f.index == index)
            .map(|f| f.name.as_str())
    }
}

// ============================================================================
// Core Response (server → client)
// ============================================================================

/// Response message sent from emb-core to emb-public
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CoreResponse {
    /// Ping response
    Pong,

    /// Serial port response
    Serial(SerialResponse),

    /// Motion planning response
    Motion(MotionResponse),

    /// Configuration response
    Config(ConfigResponse),

    /// GPIO response
    Gpio(GpioResponse),

    /// Status response
    Status(StatusResponse),

    /// Error response (used for all error cases)
    Error(ErrorInfo),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GpioResponse {
    SetPinResult {
        success: bool,
        error: Option<String>,
    },
    QueryPinResult {
        name: String,
        value: f32,
        success: bool,
    },
    /// GPIO主动上报（服务端推送）
    PinReport {
        name: String,
        value: f32,
    },
    /// 订阅结果
    SubscribeResult {
        success: bool,
        error: Option<String>,
    },
}

/// GPIO事件（用于实时推送）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GpioEvent {
    /// GPIO查询响应
    Response {
        name: String,
        value: f32,
        success: bool,
    },
    /// GPIO主动上报
    Report {
        name: String,
        value: f32,
    },
}

/// Serial port related responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SerialResponse {
    /// Connection result
    ConnectResult {
        success: bool,
        error: Option<String>,
    },
    /// Frame send result
    FrameSendResult {
        success: bool,
        error: Option<String>,
    },
    /// Received frame data
    FrameReceived {
        frame_type: u8,
        payload: Vec<u8>,
    },
    /// No frame available
        NoFrame,
    /// Serial status info
        SerialStatusInfo {
        connected: bool,
        port: Option<String>,
    },
    /// Config send result
    ConfigResult {
        success: bool,
        error: Option<String>,
    },
    /// ACK received from device
    AckReceived,
    /// NACK received from device
    NackReceived {
        error_code: u8,
    },
    /// Sequence init result
    SeqInitResult,
    /// 状态订阅结果
    SubscribeStatusResult {
        success: bool,
        error: Option<String>,
    },
    /// 状态上报推送（服务端主动推送）
    StatusReport {
        frame_type: u8,
        payload: Vec<u8>,
    },
}

/// Motion planning related responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MotionResponse {
    /// Dispatch result: segments planned, converted, and sent to device
    DispatchResult {
        success: bool,
        segments_dispatched: usize,
        error: Option<String>,
    },
    /// M指令执行结果
    MCommandResult {
        success: bool,
        error: Option<String>,
    },
    /// Current position
    PositionResult {
        x: f32,
        y: f32,
        z: f32,
        e: f32,
    },
    /// Operation acknowledged
    Acknowledged,
    /// Motion drain complete
    DrainResult {
        success: bool,
        error: Option<String>,
    },
    /// Motion and serial statistics
    Stats(MotionStatsResponse),
    /// Motor enable/disable result
    MotorEnableResult {
        success: bool,
        error: Option<String>,
    },
}

/// Motion and serial statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionStatsResponse {
    /// Serial communication statistics
    pub serial: SerialStats,
    /// Motion planning summary
    pub motion: MotionSummary,
    /// Time statistics
    pub time: TimeStats,
}

/// Serial communication statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub frames_sent: u64,
    pub frames_received: u64,
    pub crc_errors: u64,
    pub frames_invalid: u64,
    pub connected_since_ms: Option<u64>,
}

/// Motion planning summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionSummary {
    pub total_gcode_lines: u64,
    pub total_batches: u64,
    pub total_steps: u64,
    pub distance_x_mm: f64,
    pub distance_y_mm: f64,
    pub distance_z_mm: f64,
    pub distance_e_mm: f64,
    pub peak_speed_mm_per_s: f64,
    pub avg_speed_mm_per_s: f64,
    pub flow_control_wait_count: u64,
}

/// Time statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeStats {
    /// Total print time calculated from host timestamps (ms)
    pub total_print_time_ms: u64,
    /// Flow control wait time (ms)
    pub flow_control_wait_ms: u64,
    /// First move timestamp from host (ms since program start)
    pub first_move_ts_ms: Option<u64>,
    /// Last move timestamp from host (ms since program start)
    pub last_move_ts_ms: Option<u64>,
    /// Device-side first step tick (microseconds from device boot/motion start)
    pub device_first_step_tick: Option<u32>,
    /// Device-side last step tick (microseconds)
    pub device_last_step_tick: Option<u32>,
    /// Device-side motion duration (ms), calculated from device ticks
    pub device_motion_duration_ms: Option<u64>,
}

/// Configuration related responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigResponse {
    /// Config load result
    ConfigLoaded {
        success: bool,
        error: Option<String>,
    },
    /// Motion config update result
    MotionConfigUpdated {
        success: bool,
        error: Option<String>,
    },
    /// Fan config update result
    FanConfigUpdated {
        success: bool,
        error: Option<String>,
    },
    /// All configs load result
    AllConfigsLoaded {
        success: bool,
        printer_loaded: bool,
        motion_loaded: bool,
        hardware_loaded: bool,
        error: Option<String>,
    },
}

/// Status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StatusResponse {
    /// Server status info
    ServerStatus {
        version: String,
        serial_connected: bool,
        uptime_secs: u64,
    },
}

/// Error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    /// Error code
    pub code: ErrorCode,
    /// Human-readable error message
    pub message: String,
}

/// Error codes
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorCode {
    /// Invalid request format
    InvalidRequest,
    /// Serial port error
    SerialError,
    /// Motion planning error
    MotionError,
    /// Configuration error
    ConfigError,
    /// Internal server error
    InternalError,
    /// Not connected (serial not open)
    NotConnected,
    /// Request timeout
    Timeout,
}

// ============================================================================
// Wire Protocol Helpers
// ============================================================================

/// Encode a CoreRequest to JSON bytes with length prefix
pub fn encode_request(request: &CoreRequest) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec(request)
        .map_err(|e| format!("Failed to serialize request: {}", e))?;
    let len = json.len() as u32;
    let mut buf = Vec::with_capacity(4 + json.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&json);
    Ok(buf)
}

/// Decode a CoreRequest from JSON bytes with length prefix
/// Returns (remaining_bytes, decoded_request)
pub fn decode_request(buf: &[u8]) -> Result<( &[u8], CoreRequest), String> {
    if buf.len() < 4 {
        return Err("Buffer too short for length prefix".into());
    }
    let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() < 4 + len {
        return Err("Buffer too short for payload".into());
    }
    let request: CoreRequest = serde_json::from_slice(&buf[4..4 + len])
        .map_err(|e| format!("Failed to deserialize request: {}", e))?;
    Ok((&buf[4 + len..], request))
}

/// Encode a CoreResponse to JSON bytes with length prefix
pub fn encode_response(response: &CoreResponse) -> Result<Vec<u8>, String> {
    let json = serde_json::to_vec(response)
        .map_err(|e| format!("Failed to serialize response: {}", e))?;
    let len = json.len() as u32;
    let mut buf = Vec::with_capacity(4 + json.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&json);
    Ok(buf)
}

/// Decode a CoreResponse from JSON bytes with length prefix
/// Returns (remaining_bytes, decoded_response)
pub fn decode_response(buf: &[u8]) -> Result<( &[u8], CoreResponse), String> {
    if buf.len() < 4 {
        return Err("Buffer too short for length prefix".into());
    }
    let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() < 4 + len {
        return Err("Buffer too short for payload".into());
    }
    let response: CoreResponse = serde_json::from_slice(&buf[4..4 + len])
        .map_err(|e| format!("Failed to deserialize response: {}", e))?;
    Ok((&buf[4 + len..], response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping_pong_roundtrip() {
        let req = CoreRequest::Ping;
        let encoded = encode_request(&req).unwrap();

        let (remaining, decoded) = decode_request(&encoded).unwrap();
        assert!(remaining.is_empty());
        assert!(matches!(decoded, CoreRequest::Ping));

        let resp = CoreResponse::Pong;
        let resp_encoded = encode_response(&resp).unwrap();
        let (remaining, decoded_resp) = decode_response(&resp_encoded).unwrap();
        assert!(remaining.is_empty());
        assert!(matches!(decoded_resp, CoreResponse::Pong));
    }

    #[test]
    fn test_multiple_messages_in_buffer() {
        let req1 = CoreRequest::Ping;
        let req2 = CoreRequest::QueryStatus;
        let enc1 = encode_request(&req1).unwrap();
        let enc2 = encode_request(&req2).unwrap();

        let mut buf = Vec::new();
        buf.extend_from_slice(&enc1);
        buf.extend_from_slice(&enc2);

        let (remaining, decoded1) = decode_request(&buf).unwrap();
        assert!(matches!(decoded1, CoreRequest::Ping));

        let (remaining, decoded2) = decode_request(remaining).unwrap();
        assert!(remaining.is_empty());
        assert!(matches!(decoded2, CoreRequest::QueryStatus));
    }
}
