//! Configuration Adapter
//!
//! Reads `hardware.json`, `motion.json`, and `printer.json`, merges them into:
//!   1) A `MotionConfig` suitable for emb-core-server (for motion planning)
//!   2) A `PrinterJsonConfig` suitable for ConfigFrameBuilder (for STM32 device config)
//!
//! Data flow:
//!   hardware.json (per-axis: steps_per_mm, max_speed, max_accel, driver pins)
//!       +
//!   motion.json (global: velocity/acceleration, profiles, arc, homing, ...)
//!       +
//!   printer.json (communication, gcode_settings, printer params)
//!       ↓
//!   MotionConfig → send to server via CoreSocketClient::config_update_motion()
//!   PrinterJsonConfig → send to STM32 via ConfigFrameBuilder

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use super::printer_config as pc;
use super::log_config::LogConfig;
use crate::safety::config::SafetyConfig;
use crate::CoreSocketClient;
use super::config_protocol::ConfigFrameBuilder;

// ── hardware.json structures ──────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HardwareConfig {
    #[serde(default)]
    #[allow(dead_code)]
    pub version: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub description: Option<String>,
    pub communication: Option<CommunicationConfig>,
    pub limit_switch: Option<LimitSwitchHardwareConfig>,
    pub motor: Vec<MotorConfig>,
    pub gpio: Option<GpioConfig>,
    pub temperature: Option<TemperatureHardwareConfig>,
    pub heater: Option<HeaterHardwareConfig>,
    pub fan: Option<Vec<FanHardwareConfig>>,
    #[serde(default)]
    pub bed_mesh: Option<BedMeshHardwareConfig>,
}

/// Limit switch & homing hardware configuration (from hardware.json)
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LimitSwitchHardwareConfig {
    pub x: LimitSwitchAxisHardware,
    pub y: LimitSwitchAxisHardware,
    pub z: LimitSwitchAxisHardware,
    #[serde(default)]
    pub homing: LimitSwitchHomingHardware,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LimitSwitchAxisHardware {
    pub pin: String,
    #[serde(default)]
    pub pull: String,
    #[serde(default)]
    pub active_high: bool,
    #[serde(rename = "position_endstop", default)]
    pub position_endstop: Option<f32>,
    #[serde(rename = "homing_speed_mm_per_s", default = "default_homing_speed")]
    pub homing_speed_mm_per_s: u16,
    #[serde(rename = "homing_fine_speed_mm_per_s", default = "default_homing_fine_speed")]
    pub homing_fine_speed_mm_per_s: u16,
    #[serde(rename = "homing_retract_mm", default = "default_homing_retract")]
    pub homing_retract_mm: f32,
    #[serde(rename = "homing_dir", default)]
    pub homing_dir: u8,
}

fn default_homing_speed() -> u16 { 25 }
fn default_homing_fine_speed() -> u16 { 1 }
fn default_homing_retract() -> f32 { 5.0 }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LimitSwitchHomingHardware {
    #[serde(default = "default_z_lift_mm")]
    pub z_lift_mm: f32,
}

fn default_z_lift_mm() -> f32 { 10.0 }

impl Default for LimitSwitchHomingHardware {
    fn default() -> Self {
        Self { z_lift_mm: 10.0 }
    }
}

/// Fan hardware configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FanHardwareConfig {
    pub index: u8,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TemperatureHardwareConfig {
    pub hotbed: TempSensorHardwareConfig,
    pub hotend: TempSensorHardwareConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TempSensorHardwareConfig {
    pub sensor_type: String,
    pub adc_pin: String,
    pub beta: u32,
    pub pullup_resistor: u32,
    pub min_temp: i16,
    pub max_temp: u16,
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
    pub pid_interval_ms: u16,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HeaterHardwareConfig {
    pub hotbed: HeaterHardwarePin,
    pub hotend: HeaterHardwarePin,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HeaterHardwarePin {
    pub pin: String,
    pub active_high: bool,
    pub pwm_freq_hz: u16,
    pub max_power: u8,
    pub safety: HeaterSafetyHardwareConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HeaterSafetyHardwareConfig {
    pub max_temp_deviation: i16,
    pub min_temp_deviation: i16,
    pub heating_timeout_ms: u32,
    pub sensor_fault_threshold: u16,
}

// ── Bed Mesh Hardware Configuration ──────────────────────────

/// Bed mesh configuration (from hardware.json)
/// Contains probe parameters, interpolation algorithm settings, and mesh data.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BedMeshHardwareConfig {
    /// Probe parameters for mesh generation
    #[serde(default)]
    pub probe: ProbeHardwareConfig,
    /// Interpolation algorithm settings
    #[serde(default)]
    pub algorithm: BedMeshAlgorithmConfig,
    /// Mesh data (actual Z offset points)
    #[serde(default)]
    pub data: MeshDataConfig,
}

/// Probe parameters for mesh generation
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProbeHardwareConfig {
    /// Number of probe points in X direction (1-8)
    #[serde(default = "default_probe_count")]
    pub probe_count_x: u8,
    /// Number of probe points in Y direction (1-8)
    #[serde(default = "default_probe_count")]
    pub probe_count_y: u8,
    /// Number of samples per probe point
    #[serde(default = "default_probe_samples")]
    pub probe_samples: u8,
    /// Travel speed during probing (mm/s)
    #[serde(default = "default_travel_speed")]
    pub travel_speed: f32,
    /// Mesh smoothing factor for X axis
    #[serde(default = "default_mesh_smooth")]
    pub mesh_smooth_x: f32,
    /// Mesh smoothing factor for Y axis
    #[serde(default = "default_mesh_smooth")]
    pub mesh_smooth_y: f32,
    /// Minimum X coordinate of mesh area (mm)
    #[serde(default = "default_mesh_min")]
    pub mesh_min_x: f32,
    /// Maximum X coordinate of mesh area (mm)
    #[serde(default = "default_mesh_max")]
    pub mesh_max_x: f32,
    /// Minimum Y coordinate of mesh area (mm)
    #[serde(default = "default_mesh_min")]
    pub mesh_min_y: f32,
    /// Maximum Y coordinate of mesh area (mm)
    #[serde(default = "default_mesh_max")]
    pub mesh_max_y: f32,
}

fn default_enabled() -> bool { true }

fn default_probe_count() -> u8 { 5 }
fn default_probe_samples() -> u8 { 3 }
fn default_travel_speed() -> f32 { 100.0 }
fn default_mesh_smooth() -> f32 { 0.2 }
fn default_mesh_min() -> f32 { 0.0 }
fn default_mesh_max() -> f32 { 235.0 }

impl Default for ProbeHardwareConfig {
    fn default() -> Self {
        Self {
            probe_count_x: default_probe_count(),
            probe_count_y: default_probe_count(),
            probe_samples: default_probe_samples(),
            travel_speed: default_travel_speed(),
            mesh_smooth_x: default_mesh_smooth(),
            mesh_smooth_y: default_mesh_smooth(),
            mesh_min_x: default_mesh_min(),
            mesh_max_x: default_mesh_max(),
            mesh_min_y: default_mesh_min(),
            mesh_max_y: default_mesh_max(),
        }
    }
}

/// Interpolation algorithm settings
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BedMeshAlgorithmConfig {
    /// Enable/disable interpolation algorithm (default: true)
    /// When disabled, bed mesh compensation is skipped — useful for debugging
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Interpolation algorithm: "lagrange", "bicubic", or "bilinear"
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
    /// Bicubic tension parameter (0.0-1.0)
    #[serde(default = "default_bicubic_tension")]
    pub bicubic_tension: f32,
    /// Mesh interpolation points per segment in X direction
    #[serde(default = "default_mesh_pps")]
    pub mesh_pps_x: u8,
    /// Mesh interpolation points per segment in Y direction
    #[serde(default = "default_mesh_pps")]
    pub mesh_pps_y: u8,
    /// Probe Z offset (persistent, mm)
    #[serde(default)]
    pub probe_z_adjust: f32,
    /// Fade start layer (full compensation above this layer)
    #[serde(default = "default_fade_start")]
    pub fade_start: f32,
    /// Fade end layer (no compensation below this layer)
    #[serde(default = "default_fade_end")]
    pub fade_end: f32,
}

fn default_algorithm() -> String { "bicubic".to_string() }
fn default_bicubic_tension() -> f32 { 0.5 }
fn default_mesh_pps() -> u8 { 1 }
fn default_fade_start() -> f32 { 5.0 }
fn default_fade_end() -> f32 { 10.0 }

impl Default for BedMeshAlgorithmConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            algorithm: default_algorithm(),
            bicubic_tension: default_bicubic_tension(),
            mesh_pps_x: default_mesh_pps(),
            mesh_pps_y: default_mesh_pps(),
            probe_z_adjust: 0.0,
            fade_start: default_fade_start(),
            fade_end: default_fade_end(),
        }
    }
}

impl Default for BedMeshHardwareConfig {
    fn default() -> Self {
        Self {
            probe: ProbeHardwareConfig::default(),
            algorithm: BedMeshAlgorithmConfig::default(),
            data: MeshDataConfig::default(),
        }
    }
}

/// Mesh data configuration (from hardware.json bed_mesh.data)
/// Contains actual mesh points.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MeshDataConfig {
    /// Z offset values (y_count × x_count, row-major order)
    #[serde(default)]
    pub points: Vec<f32>,
}

impl Default for MeshDataConfig {
    fn default() -> Self {
        Self {
            points: vec![0.0; 25], // Default 5×5 grid with all zeros
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GpioConfig {
    pub output: Vec<OutputGpioConfig>,
    pub input: Vec<InputGpioConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OutputGpioConfig {
    pub name: String,
    pub pin: String,
    #[serde(rename = "type")]
    pub pin_type: String,
    #[serde(default = "default_true")]
    pub active_high: bool,
    #[serde(default)]
    pub pwm_freq_hz: u32,
    #[serde(default)]
    pub default_value: f32,
    #[serde(default)]
    pub shutdown_value: f32,
    #[serde(default = "default_max_value")]
    pub max_value: f32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InputGpioConfig {
    pub name: String,
    pub pin: String,
    #[serde(rename = "type")]
    pub pin_type: String,
    #[serde(default)]
    pub pull: String,
    #[serde(default = "default_true")]
    pub active_high: bool,
    #[serde(default)]
    pub debounce_ms: u16,
    pub event: Option<InputGpioEvent>,
    pub report: Option<InputGpioReport>,
    pub calibration: Option<InputGpioCalibration>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InputGpioEvent {
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub action: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InputGpioReport {
    #[serde(default)]
    pub mode: String,
    pub trigger: Option<String>,
    pub interval_ms: Option<u16>,
    pub threshold: Option<f32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InputGpioCalibration {
    #[serde(default)]
    pub offset: f32,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default)]
    pub min: f32,
    #[serde(default = "default_max_value")]
    pub max: f32,
}

fn default_true() -> bool { true }
fn default_max_value() -> f32 { 1.0 }
fn default_scale() -> f32 { 1.0 }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CommunicationConfig {
    pub serial: Option<SerialConfig>,
    /// 状态上报间隔（毫秒），可选
    #[serde(default)]
    pub status_report_interval_ms: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SerialConfig {
    pub port: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: String,
    pub stop_bits: u8,
    pub timeout_ms: u64,
    pub flow_control: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MotorConfig {
    pub axis: String,
    pub step_pin: String,
    pub dir_pin: String,
    pub enable_pin: String,
    pub steps_per_mm: f32,
    /// Per-axis speed limit (user can independently restrict each axis).
    pub max_speed_mm_per_s: f32,
    /// Per-axis acceleration limit (user can independently restrict each axis).
    pub max_accel: f32,
    #[serde(default)]
    pub position_min: f32,
    #[serde(default)]
    pub position_max: f32,
    pub driver: Option<DriverConfig>,
    pub extruder: Option<ExtruderConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DriverConfig {
    pub uart_pin: String,
    pub microsteps: u16,
    pub current_ma: u16,
    pub hold_current_ma: u16,
    pub stealthchop_threshold: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExtruderConfig {
    pub nozzle_diameter_mm: f32,
    pub filament_diameter_mm: f32,
    pub max_flow_rate: f32,
}

// ── motion.json structures ───────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MotionFileConfig {
    #[serde(default)]
    pub kinematics: KinematicsSection,
    #[serde(default)]
    pub junction: JunctionSection,
    #[serde(default)]
    pub segment: SegmentSection,
    #[serde(default)]
    pub homing: HomingSection,
    #[serde(default)]
    pub arc: ArcSection,
    #[serde(default)]
    pub extruder: ExtruderMotionSection,
    #[serde(default)]
    pub velocity_profile: VelocityProfileFile,
    #[serde(default)]
    pub resonance_compensation: ResonanceCompensationSection,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct KinematicsSection {
    #[serde(default)]
    pub max_velocity: f32,
    #[serde(default)]
    pub max_acceleration: f32,
    #[serde(default)]
    pub max_feed_rate: f32,
    #[serde(default)]
    pub jerk: f32,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct JunctionSection {
    #[serde(default)]
    pub square_corner_velocity: f32,
    #[serde(default)]
    pub junction_deviation: f32,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct SegmentSection {
    #[serde(default)]
    pub segment_time_ms: u16,
    #[serde(default)]
    pub min_segment_distance: f32,
    #[serde(default)]
    pub buffer_ahead_ms: u16,
    #[serde(default = "default_true")]
    pub microstep_accumulation_enabled: bool,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct HomingDirection {
    #[serde(default)]
    pub x: i8,
    #[serde(default)]
    pub y: i8,
    #[serde(default)]
    pub z: i8,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct HomingSection {
    #[serde(default)]
    pub speed: f32,
    #[serde(default)]
    pub retract_speed: f32,
    #[serde(default)]
    pub backoff: f32,
    #[serde(default)]
    pub direction: HomingDirection,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ArcSection {
    #[serde(default)]
    pub sag_tolerance: f32,
    #[serde(default)]
    pub centripetal_accel: f32,
    #[serde(default)]
    pub min_segments: u32,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ExtruderMotionSection {
    #[serde(default)]
    pub pressure_advance: f32,
    #[serde(default)]
    pub pressure_advance_max_accel: f32,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct VelocityProfileFile {
    #[serde(default)]
    #[allow(dead_code)]
    pub r#type: String,
    pub six_point: Option<SixPointFile>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResonanceCompensationSection {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_shaper_type")]
    pub shaper_type: String,
    /// Insensitivity parameter for EI shapers (0.0–0.25, default 0.05).
    /// Higher values provide wider suppression bandwidth but may introduce
    /// more delay. Only affects EI, 2-hump EI, and 3-hump EI shapers.
    #[serde(default = "default_insensitivity")]
    pub insensitivity: f32,
    #[serde(default)]
    pub x: AxisResonanceFile,
    #[serde(default)]
    pub y: AxisResonanceFile,
    /// Maximum velocity jump allowed between adjacent shaped sub-segments (mm/s).
    /// Smooths ZV/ZVD sub-segment boundaries to prevent stepper driver jerk.
    /// Higher = more aggressive shaping; lower = smoother transitions.
    #[serde(default = "default_max_velocity_jump_mm_s")]
    pub max_velocity_jump_mm_s: f32,
}

impl Default for ResonanceCompensationSection {
    fn default() -> Self {
        Self {
            enabled: false,
            shaper_type: default_shaper_type(),
            insensitivity: default_insensitivity(),
            x: AxisResonanceFile::default(),
            y: AxisResonanceFile::default(),
            max_velocity_jump_mm_s: default_max_velocity_jump_mm_s(),
        }
    }
}

fn default_shaper_type() -> String { "ZV".to_string() }
fn default_resonance_frequency() -> f32 { 45.0 }
fn default_resonance_damping() -> f32 { 0.1 }
fn default_max_velocity_jump_mm_s() -> f32 { 10.0 }
fn default_insensitivity() -> f32 { 0.05 }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AxisResonanceFile {
    #[serde(default = "default_resonance_frequency")]
    pub frequency: f32,
    #[serde(default = "default_resonance_damping")]
    pub damping: f32,
}

impl Default for AxisResonanceFile {
    fn default() -> Self {
        Self {
            frequency: default_resonance_frequency(),
            damping: default_resonance_damping(),
        }
    }
}

// ── motion.json structures (continued) ───────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SixPointFile {
    pub start_accel_mm_s2: f32,
    pub max_accel_mm_s2: f32,
    pub final_decel_mm_s2: f32,
    pub max_decel_mm_s2: f32,
    pub start_speed_mm_s: f32,
    pub stop_speed_mm_s: f32,
    pub break_speed_mm_s: f32,
    pub min_distance_mm: f32,
}

// ── temperature.json structures ───────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TemperatureFileConfig {
    #[allow(dead_code)]
    pub version: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    pub presets: Vec<TemperaturePresetFile>,
    pub safety: TemperatureSafetyFile,
    pub wait: TemperatureWaitFile,
    pub auto_fan: AutoFanFile,
    pub pid_tune: PidTuneFile,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TemperaturePresetFile {
    pub name: String,
    pub hotend_temp: f32,
    pub bed_temp: f32,
    pub chamber_temp: Option<f32>,
    pub fan_speed: u8,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TemperatureSafetyFile {
    pub safety_check_interval_ms: u32,
    pub temp_change_threshold: f32,
    // (heaters safety config moved to config/safety.json)
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TemperatureWaitFile {
    pub timeout_secs: u32,
    pub tolerance: f32,
    #[serde(default = "default_wait_check_interval_ms")]
    pub check_interval_ms: u64,
    #[serde(default = "default_wait_stable_count")]
    pub stable_count: u32,
}

fn default_wait_check_interval_ms() -> u64 { 500 }
fn default_wait_stable_count() -> u32 { 3 }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AutoFanFile {
    pub enable: bool,
    pub gpio_name: String,
    pub on_threshold: f32,
    pub off_threshold: f32,
    pub on_value: f32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PidTuneFile {
    pub default_cycles: u8,
    pub hotend: PidTuneHeaterFile,
    pub hotbed: PidTuneHeaterFile,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PidTuneHeaterFile {
    pub max_overtemp: u8,
    pub timeout_ms: u32,
    pub power_divisor: u16,
    pub switch_delay_ms: u32,
    pub initial_bias: u8,
    pub initial_d: u8,
}

// ── printer.json structures ───────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PrinterFileConfig {
    #[allow(dead_code)]
    pub version: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub description: Option<String>,
    #[allow(dead_code)]
    pub printer_model: String,
    #[allow(dead_code)]
    pub communication: Option<CommunicationConfig>,
    #[allow(dead_code)]
    pub printer: Option<PrinterParamsSection>,
    #[allow(dead_code)]
    pub gcode_settings: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PrinterParamsSection {
    pub max_velocity: f32,
    pub max_acceleration: f32,
    pub square_corner_velocity: f32,
    pub junction_deviation: f32,
    pub velocity_profile: Option<VelocityProfileFile>,
}

// ── Public API ───────────────────────────────────────────────

/// Read and parse all config files from the given directory.
pub fn load_configs(config_dir: &str) -> Result<LoadedConfigs, String> {
    let hw_path = format!("{}/hardware.json", config_dir);
    let mo_path = format!("{}/motion.json", config_dir);
    let pr_path = format!("{}/printer.json", config_dir);
    let tp_path = format!("{}/temperature.json", config_dir);
    let sf_path = format!("{}/safety.json", config_dir);
    let lg_path = format!("{}/logging.json", config_dir);

    let hw_str = std::fs::read_to_string(&hw_path)
        .map_err(|e| format!("Failed to read {}: {}", hw_path, e))?;
    let mo_str = std::fs::read_to_string(&mo_path)
        .map_err(|e| format!("Failed to read {}: {}", mo_path, e))?;
    let pr_str = std::fs::read_to_string(&pr_path)
        .map_err(|e| format!("Failed to read {}: {}", pr_path, e))?;
    let tp_str = std::fs::read_to_string(&tp_path)
        .map_err(|e| format!("Failed to read {}: {}", tp_path, e))?;
    let sf_str = std::fs::read_to_string(&sf_path)
        .map_err(|e| format!("Failed to read {}: {}", sf_path, e))?;
    let lg_str = std::fs::read_to_string(&lg_path)
        .map_err(|e| format!("Failed to read {}: {}", lg_path, e))?;

    let hardware: HardwareConfig = serde_json::from_str(&hw_str)
        .map_err(|e| format!("Parse {} error: {}", hw_path, e))?;
    let motion: MotionFileConfig = serde_json::from_str(&mo_str)
        .map_err(|e| format!("Parse {} error: {}", mo_path, e))?;
    let printer: PrinterFileConfig = serde_json::from_str(&pr_str)
        .map_err(|e| format!("Parse {} error: {}", pr_path, e))?;
    let temperature: TemperatureFileConfig = serde_json::from_str(&tp_str)
        .map_err(|e| format!("Parse {} error: {}", tp_path, e))?;
    let safety: SafetyConfig = serde_json::from_str(&sf_str)
        .map_err(|e| format!("Parse {} error: {}", sf_path, e))?;
    let logging: LogConfig = serde_json::from_str(&lg_str)
        .map_err(|e| format!("Parse {} error: {}", lg_path, e))?;

    Ok(LoadedConfigs { hardware, motion, printer, temperature, safety, logging })
}

/// All loaded configuration data.
#[derive(Clone)]
#[allow(dead_code)]
pub struct LoadedConfigs {
    pub hardware: HardwareConfig,
    pub motion: MotionFileConfig,
    pub printer: PrinterFileConfig,
    pub temperature: TemperatureFileConfig,
    pub safety: SafetyConfig,
    pub logging: LogConfig,
}

/// Merge hardware per-axis values + motion global values into a single JSON
/// string representing `MotionConfig`, ready to send to the server.
pub fn build_motion_config_json(configs: &LoadedConfigs) -> Result<String, String> {
    // Build per-axis maps from motor[]
    let mut axis_map: HashMap<String, &MotorConfig> = HashMap::new();
    for motor in &configs.hardware.motor {
        axis_map.insert(motor.axis.to_uppercase(), motor);
    }

    // Extract per-axis values (with defaults for missing axes)
    let x = axis_map.get("X");
    let y = axis_map.get("Y");
    let z = axis_map.get("Z");
    let e = axis_map.get("E0").or_else(|| axis_map.get("E"));

    let x_steps_per_mm = x.map(|m| m.steps_per_mm).unwrap_or(80.0);
    let y_steps_per_mm = y.map(|m| m.steps_per_mm).unwrap_or(80.0);
    let z_steps_per_mm = z.map(|m| m.steps_per_mm).unwrap_or(400.0);
    let e_steps_per_mm = e.map(|m| m.steps_per_mm).unwrap_or(93.0);

    let x_max_speed = x.map(|m| m.max_speed_mm_per_s).unwrap_or(configs.motion.kinematics.max_velocity);
    let y_max_speed = y.map(|m| m.max_speed_mm_per_s).unwrap_or(configs.motion.kinematics.max_velocity);
    let z_max_speed = z.map(|m| m.max_speed_mm_per_s).unwrap_or(configs.motion.kinematics.max_velocity);
    let e_max_speed = e.map(|m| m.max_speed_mm_per_s).unwrap_or(50.0);

    let x_max_accel = x.map(|m| m.max_accel).unwrap_or(configs.motion.kinematics.max_acceleration);
    let y_max_accel = y.map(|m| m.max_accel).unwrap_or(configs.motion.kinematics.max_acceleration);
    let z_max_accel = z.map(|m| m.max_accel).unwrap_or(500.0);
    let e_max_accel = e.map(|m| m.max_accel).unwrap_or(5000.0);

    // Axis position bounds (from hardware.json motor position_min/max)
    let x_position_min = x.map(|m| m.position_min).unwrap_or(0.0);
    let x_position_max = x.map(|m| m.position_max).unwrap_or(0.0);
    let y_position_min = y.map(|m| m.position_min).unwrap_or(0.0);
    let y_position_max = y.map(|m| m.position_max).unwrap_or(0.0);
    let z_position_min = z.map(|m| m.position_min).unwrap_or(0.0);
    let z_position_max = z.map(|m| m.position_max).unwrap_or(0.0);

    // Velocity profile
    let vp = &configs.motion.velocity_profile;
    let six = vp.six_point.as_ref();

    let mut json = serde_json::json!({
        "x_steps_per_mm": x_steps_per_mm,
        "y_steps_per_mm": y_steps_per_mm,
        "z_steps_per_mm": z_steps_per_mm,
        "e_steps_per_mm": e_steps_per_mm,
        "max_velocity": configs.motion.kinematics.max_velocity,
        "x_max_speed": x_max_speed,
        "y_max_speed": y_max_speed,
        "z_max_speed": z_max_speed,
        "e_max_speed": e_max_speed,
        "max_acceleration": configs.motion.kinematics.max_acceleration,
        "x_max_accel": x_max_accel,
        "y_max_accel": y_max_accel,
        "z_max_accel": z_max_accel,
        "e_max_accel": e_max_accel,
        "jerk": configs.motion.kinematics.jerk,
        "square_corner_velocity": configs.motion.junction.square_corner_velocity,
        "junction_deviation": configs.motion.junction.junction_deviation,
        "max_feed_rate": configs.motion.kinematics.max_feed_rate,
        "segment_time_ms": configs.motion.segment.segment_time_ms,
        "min_segment_distance": configs.motion.segment.min_segment_distance,
        "buffer_ahead_ms": configs.motion.segment.buffer_ahead_ms,
        "microstep_accumulation_enabled": configs.motion.segment.microstep_accumulation_enabled,
        "x_position_min": x_position_min,
        "x_position_max": x_position_max,
        "y_position_min": y_position_min,
        "y_position_max": y_position_max,
        "z_position_min": z_position_min,
        "z_position_max": z_position_max,
        "homing_speed": Some(configs.motion.homing.speed),
        "homing_retract_speed": Some(configs.motion.homing.retract_speed),
        "homing_backoff": Some(configs.motion.homing.backoff),
        "homing_direction_x": Some(configs.motion.homing.direction.x),
        "homing_direction_y": Some(configs.motion.homing.direction.y),
        "homing_direction_z": Some(configs.motion.homing.direction.z),
        "arc_sag_tolerance": configs.motion.arc.sag_tolerance,
        "arc_centripetal_accel": configs.motion.arc.centripetal_accel,
        "arc_min_segments": configs.motion.arc.min_segments,
        "pressure_advance": configs.motion.extruder.pressure_advance,
        "pressure_advance_max_accel": configs.motion.extruder.pressure_advance_max_accel,
        "velocity_profile_type": vp.r#type.as_str(),
    });

    if let Some(sp) = six {
        json["six_point_start_accel"] = serde_json::json!(sp.start_accel_mm_s2);
        json["six_point_max_accel"] = serde_json::json!(sp.max_accel_mm_s2);
        json["six_point_final_decel"] = serde_json::json!(sp.final_decel_mm_s2);
        json["six_point_max_decel"] = serde_json::json!(sp.max_decel_mm_s2);
        json["six_point_start_speed"] = serde_json::json!(sp.start_speed_mm_s);
        json["six_point_stop_speed"] = serde_json::json!(sp.stop_speed_mm_s);
        json["six_point_break_speed"] = serde_json::json!(sp.break_speed_mm_s);
        json["six_point_min_distance"] = serde_json::json!(sp.min_distance_mm);
    }

    // Resonance compensation
    json["resonance_compensation"] = serde_json::json!({
        "enabled": configs.motion.resonance_compensation.enabled,
        "shaper_type": configs.motion.resonance_compensation.shaper_type,
        "insensitivity": configs.motion.resonance_compensation.insensitivity,
        "max_velocity_jump_mm_s": configs.motion.resonance_compensation.max_velocity_jump_mm_s,
        "x": {
            "frequency": configs.motion.resonance_compensation.x.frequency,
            "damping": configs.motion.resonance_compensation.x.damping,
        },
        "y": {
            "frequency": configs.motion.resonance_compensation.y.frequency,
            "damping": configs.motion.resonance_compensation.y.damping,
        },
    });

    serde_json::to_string_pretty(&json)
        .map_err(|e| format!("Serialize MotionConfig JSON failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_and_merge() {
        let configs = load_configs("config").expect("load configs");
        assert_eq!(configs.hardware.motor.len(), 4);

        let json = build_motion_config_json(&configs).expect("merge");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Per-axis values from hardware.json
        assert_eq!(v["x_steps_per_mm"], 80.0);
        assert_eq!(v["z_steps_per_mm"], 400.0);
        assert_eq!(v["x_max_speed"], 400.0);
        assert_eq!(v["z_max_speed"], 20.0);
        assert_eq!(v["x_max_accel"], 20000.0);
        assert_eq!(v["z_max_accel"], 500.0);

        // Global values from motion.json
        assert_eq!(v["max_velocity"], 400.0);
        assert_eq!(v["junction_deviation"], 0.05);
        assert_eq!(v["six_point_max_accel"], 20000.0);
    }
}

/// Merge hardware/motion/printer configs into a PrinterJsonConfig suitable for ConfigFrameBuilder.
pub fn build_printer_config(configs: &LoadedConfigs) -> pc::PrinterJsonConfig {
    // Build motors
    let motors: Vec<pc::MotorParams> = configs.hardware.motor.iter().map(|m| {
        pc::MotorParams {
            axis: m.axis.clone(),
            step_pin: m.step_pin.clone(),
            dir_pin: m.dir_pin.clone(),
            enable_pin: m.enable_pin.clone(),
            max_speed_mm_per_s: m.max_speed_mm_per_s as u16,
            max_accel: m.max_accel as u32,
            steps_per_mm: m.steps_per_mm as u32,
            position_min: m.position_min as i32,
            position_max: m.position_max as i32,
            driver: m.driver.as_ref().map(|d| pc::DriverParams {
                uart_pin: d.uart_pin.clone(),
                microsteps: d.microsteps as u8,
                current_ma: d.current_ma,
                hold_current_ma: d.hold_current_ma,
                stealthchop_threshold: d.stealthchop_threshold,
            }).unwrap_or_default(),
            extruder: m.extruder.as_ref().map(|e| pc::ExtruderParams {
                nozzle_diameter_mm: Some(e.nozzle_diameter_mm),
                filament_diameter_mm: Some(e.filament_diameter_mm),
                max_flow_rate: Some(e.max_flow_rate),
            }).unwrap_or_default(),
        }
    }).collect();

    // Build communication config
    let comm = configs.printer.communication.as_ref()
        .or_else(|| configs.hardware.communication.as_ref());

    let communication = pc::CommunicationConfig {
        serial: comm.and_then(|c| c.serial.as_ref())
            .map(|s| pc::SerialPortConfig {
                port: s.port.clone(),
                baud_rate: s.baud_rate,
                data_bits: s.data_bits,
                parity: s.parity.clone(),
                stop_bits: s.stop_bits,
                timeout_ms: s.timeout_ms as u32,
                flow_control: s.flow_control,
            })
            .unwrap_or_default(),
        status_report_interval_ms: comm.and_then(|c| c.status_report_interval_ms).unwrap_or(1000),
    };

    // Build printer params
    let printer_params = configs.printer.printer.as_ref()
        .map(|p| {
            let vp = p.velocity_profile.as_ref()
                .or_else(|| configs.motion.velocity_profile.six_point.as_ref().map(|_| &configs.motion.velocity_profile));
            
            let velocity_profile = vp.and_then(|vp| {
                match vp.r#type.as_str() {
                    "six_point" | "SixPoint" => {
                        let sp = vp.six_point.as_ref()?;
                        Some(pc::VelocityProfileConfig::SixPoint {
                            six_point: pc::SixPointConfig {
                                start_accel_mm_s2: sp.start_accel_mm_s2,
                                max_accel_mm_s2: sp.max_accel_mm_s2,
                                final_decel_mm_s2: sp.final_decel_mm_s2,
                                max_decel_mm_s2: sp.max_decel_mm_s2,
                                start_speed_mm_s: sp.start_speed_mm_s,
                                stop_speed_mm_s: sp.stop_speed_mm_s,
                                break_speed_mm_s: sp.break_speed_mm_s,
                                min_distance_mm: sp.min_distance_mm,
                            },
                        })
                    }
                    "s_curve" | "SCurve" => {
                        Some(pc::VelocityProfileConfig::SCurve {
                            s_curve: Default::default(),
                        })
                    }
                    _ => Some(pc::VelocityProfileConfig::Trapezoidal),
                }
            })
            .unwrap_or_default();

            pc::PrinterParams {
                max_velocity: p.max_velocity,
                max_acceleration: p.max_acceleration,
                square_corner_velocity: p.square_corner_velocity,
                junction_deviation: p.junction_deviation,
                velocity_profile,
            }
        })
        .or_else(|| Some(pc::PrinterParams {
            max_velocity: configs.motion.kinematics.max_velocity,
            max_acceleration: configs.motion.kinematics.max_acceleration,
            square_corner_velocity: configs.motion.junction.square_corner_velocity,
            junction_deviation: configs.motion.junction.junction_deviation,
            velocity_profile: configs.motion.velocity_profile.six_point.as_ref()
                .map(|sp| pc::VelocityProfileConfig::SixPoint {
                    six_point: pc::SixPointConfig {
                        start_accel_mm_s2: sp.start_accel_mm_s2,
                        max_accel_mm_s2: sp.max_accel_mm_s2,
                        final_decel_mm_s2: sp.final_decel_mm_s2,
                        max_decel_mm_s2: sp.max_decel_mm_s2,
                        start_speed_mm_s: sp.start_speed_mm_s,
                        stop_speed_mm_s: sp.stop_speed_mm_s,
                        break_speed_mm_s: sp.break_speed_mm_s,
                        min_distance_mm: sp.min_distance_mm,
                    },
                })
                .unwrap_or_default(),
        }))
        .unwrap_or_default();

    // Build gcode_settings
    let gcode_settings = configs.printer.gcode_settings.as_ref()
        .and_then(|v| serde_json::from_value::<pc::GCodeSettings>(v.clone()).ok())
        .unwrap_or_default();

    // Build GPIO config
    let gpio = configs.hardware.gpio.as_ref().map(|hw_gpio| {
        let output_pins: Vec<pc::OutputPinParams> = hw_gpio.output.iter().map(|o| {
            pc::OutputPinParams {
                name: o.name.clone(),
                pin: o.pin.clone(),
                pin_type: match o.pin_type.as_str() {
                    "pwm" => pc::OutputPinType::Pwm,
                    _ => pc::OutputPinType::Digital,
                },
                active_high: o.active_high,
                pwm_freq_hz: o.pwm_freq_hz as u16,
                default_value: o.default_value,
                shutdown_value: o.shutdown_value,
                max_value: o.max_value,
            }
        }).collect();

        let input_pins: Vec<pc::InputPinParams> = hw_gpio.input.iter().map(|i| {
            pc::InputPinParams {
                name: i.name.clone(),
                pin: i.pin.clone(),
                pin_type: match i.pin_type.as_str() {
                    "analog" => pc::InputPinType::Analog,
                    _ => pc::InputPinType::Digital,
                },
                pull: i.pull.clone(),
                active_high: i.active_high,
                debounce_ms: i.debounce_ms,
                event: None,
                report: None,
                calibration: None,
                adc_resolution: 12, // Default ADC resolution
            }
        }).collect();

        pc::GpioConfig {
            output: output_pins,
            input: input_pins,
        }
    }).unwrap_or_default();

    // Build temperature config
    let temperature = configs.hardware.temperature.as_ref().map(|temp| {
        pc::TemperatureParams {
            hotbed: pc::TempSensorParams {
                sensor_type: temp.hotbed.sensor_type.clone(),
                adc_pin: temp.hotbed.adc_pin.clone(),
                beta: temp.hotbed.beta,
                pullup_resistor: temp.hotbed.pullup_resistor,
                min_temp: temp.hotbed.min_temp,
                max_temp: temp.hotbed.max_temp,
                kp: temp.hotbed.kp,
                ki: temp.hotbed.ki,
                kd: temp.hotbed.kd,
                pid_interval_ms: temp.hotbed.pid_interval_ms,
            },
            hotend: pc::TempSensorParams {
                sensor_type: temp.hotend.sensor_type.clone(),
                adc_pin: temp.hotend.adc_pin.clone(),
                beta: temp.hotend.beta,
                pullup_resistor: temp.hotend.pullup_resistor,
                min_temp: temp.hotend.min_temp,
                max_temp: temp.hotend.max_temp,
                kp: temp.hotend.kp,
                ki: temp.hotend.ki,
                kd: temp.hotend.kd,
                pid_interval_ms: temp.hotend.pid_interval_ms,
            },
        }
    }).unwrap_or_default();

    // Build heater config
    let heater = configs.hardware.heater.as_ref().map(|h| {
        pc::HeaterParams {
            hotbed: pc::HeaterPin {
                pin: h.hotbed.pin.clone(),
                active_high: h.hotbed.active_high,
                pwm_freq_hz: h.hotbed.pwm_freq_hz,
                max_power: h.hotbed.max_power,
                safety: pc::HeaterSafetyConfig {
                    max_temp_deviation: h.hotbed.safety.max_temp_deviation,
                    min_temp_deviation: h.hotbed.safety.min_temp_deviation,
                    heating_timeout_ms: h.hotbed.safety.heating_timeout_ms,
                    sensor_fault_threshold: h.hotbed.safety.sensor_fault_threshold,
                },
            },
            hotend: pc::HeaterPin {
                pin: h.hotend.pin.clone(),
                active_high: h.hotend.active_high,
                pwm_freq_hz: h.hotend.pwm_freq_hz,
                max_power: h.hotend.max_power,
                safety: pc::HeaterSafetyConfig {
                    max_temp_deviation: h.hotend.safety.max_temp_deviation,
                    min_temp_deviation: h.hotend.safety.min_temp_deviation,
                    heating_timeout_ms: h.hotend.safety.heating_timeout_ms,
                    sensor_fault_threshold: h.hotend.safety.sensor_fault_threshold,
                },
            },
        }
    }).unwrap_or_default();

    // Build fans
    let fans: Vec<pc::FanParams> = configs.hardware.fan.as_ref()
        .map(|fan_list| {
            let mut sorted_fans = fan_list.clone();
            sorted_fans.sort_by_key(|f| f.index);

            sorted_fans.into_iter().map(|f| {
                pc::FanParams {
                    name: f.name.clone(),
                    pin: String::new(),
                    active_high: true,
                    pwm_freq_hz: 100,
                }
            }).collect()
        })
        .unwrap_or_default();

    // Build temperature presets from temperature.json
    let temperature_presets: Vec<pc::TemperaturePresetConfig> = configs.temperature.presets.iter().map(|p| {
        pc::TemperaturePresetConfig {
            name: p.name.clone(),
            hotend_temp: p.hotend_temp,
            bed_temp: p.bed_temp,
            chamber_temp: p.chamber_temp,
            fan_speed: p.fan_speed,
        }
    }).collect();

    // Build PID tune config from temperature.json
    let pid_tune = Some(pc::PidTuneParams {
        default_cycles: configs.temperature.pid_tune.default_cycles,
        hotend: pc::PidTuneHeaterConfig {
            max_overtemp: configs.temperature.pid_tune.hotend.max_overtemp as f32,
            timeout_ms: configs.temperature.pid_tune.hotend.timeout_ms,
            power_divisor: configs.temperature.pid_tune.hotend.power_divisor as u32,
            switch_delay_ms: configs.temperature.pid_tune.hotend.switch_delay_ms,
            initial_bias: configs.temperature.pid_tune.hotend.initial_bias as u32,
            initial_d: configs.temperature.pid_tune.hotend.initial_d as u32,
        },
        hotbed: pc::PidTuneHeaterConfig {
            max_overtemp: configs.temperature.pid_tune.hotbed.max_overtemp as f32,
            timeout_ms: configs.temperature.pid_tune.hotbed.timeout_ms,
            power_divisor: configs.temperature.pid_tune.hotbed.power_divisor as u32,
            switch_delay_ms: configs.temperature.pid_tune.hotbed.switch_delay_ms,
            initial_bias: configs.temperature.pid_tune.hotbed.initial_bias as u32,
            initial_d: configs.temperature.pid_tune.hotbed.initial_d as u32,
        },
    });

    // Build temperature wait config from temperature.json
    let temperature_wait = pc::TemperatureWaitConfig {
        timeout_secs: configs.temperature.wait.timeout_secs as u64,
        tolerance: configs.temperature.wait.tolerance,
        check_interval_ms: configs.temperature.wait.check_interval_ms,
        stable_count: configs.temperature.wait.stable_count,
    };

    // Build auto-fan config from temperature.json
    let auto_fan = pc::AutoFanConfig {
        enable: configs.temperature.auto_fan.enable,
        gpio_name: configs.temperature.auto_fan.gpio_name.clone(),
        on_threshold: configs.temperature.auto_fan.on_threshold,
        off_threshold: configs.temperature.auto_fan.off_threshold,
        on_value: configs.temperature.auto_fan.on_value,
    };

    pc::PrinterJsonConfig {
        version: configs.printer.version.clone(),
        printer_model: configs.printer.printer_model.clone(),
        communication,
        printer: printer_params,
        gcode_settings,
        motor: motors,
        gpio,
        temperature,
        heater,
        fan: fans,
        limit_switch: build_limit_switch_config(&configs),
        temperature_presets,
        pid_tune,
        temperature_wait,
        auto_fan,
        ..Default::default()
    }
}

/// Build limit switch + homing config from hardware.json
fn build_limit_switch_config(configs: &LoadedConfigs) -> pc::LimitSwitchParams {
    let hw = match &configs.hardware.limit_switch {
        Some(ls) => ls,
        None => return pc::LimitSwitchParams::default(),
    };

    let map_axis = |axis: &LimitSwitchAxisHardware| -> pc::LimitSwitchAxis {
        pc::LimitSwitchAxis {
            pin: axis.pin.clone(),
            pull: axis.pull.clone(),
            active_high: axis.active_high,
            position_endstop: axis.position_endstop,
            homing_speed_mm_per_s: axis.homing_speed_mm_per_s,
            homing_fine_speed_mm_per_s: axis.homing_fine_speed_mm_per_s,
            homing_retract_mm: axis.homing_retract_mm,
            homing_dir: axis.homing_dir,
        }
    };

    pc::LimitSwitchParams {
        x: map_axis(&hw.x),
        y: map_axis(&hw.y),
        z: map_axis(&hw.z),
        homing: pc::HomingGlobalParams {
            z_lift_mm: hw.homing.z_lift_mm,
        },
    }
}

/// Initialize device with all configurations.
/// 
/// This function performs the complete device initialization process:
/// 1. Load configuration files via ConfigManager
/// 2. Connect to serial port
/// 3. Send motion config to server (for motion planning)
/// 4. Send hardware config frames to device (motor, temperature, heater, gpio, etc.)
/// 5. Send ConfigComplete to device
/// 
/// # Arguments
/// * `client` - The CoreSocketClient to use for sending data
/// * `config_dir` - Path to the configuration directory (contains printer.json, motion.json, hardware.json)
/// 
/// # Returns
/// * `Ok(())` if all configurations were sent successfully
/// * `Err(String)` if any step failed
pub async fn configure_device(client: &CoreSocketClient, config_dir: &str) -> Result<(), String> {
    use super::config_manager::ConfigManager;
    
    // Step 1: Load configuration files via ConfigManager
    ConfigManager::instance().load(config_dir)?;
    let configs = load_configs(config_dir)?;
    
    // Step 2: Connect to serial port
    // Read serial configuration from printer.json
    if let Some(comm) = &configs.printer.communication {
        if let Some(serial) = &comm.serial {
            tracing::info!("🔌 连接串口: {} @ {}", serial.port, serial.baud_rate);
            match client.serial_connect(&serial.port, serial.baud_rate).await {
                Ok(()) => tracing::info!("✅ 串口连接成功"),
                Err(e) => {
                    tracing::error!("❌ 串口连接失败: {}", e);
                    tracing::info!("💡 请确认下位机已连接到 {}", serial.port);
                    return Err(format!("串口连接失败: {}", e));
                }
            }
        }
    }
    
    // Step 3: Send motion config to server (for motion planning)
    // This includes: max_velocity, junction_deviation, velocity_profile, etc.
    let motion_config_json = build_motion_config_json(&configs)?;
    client.config_update_motion(&motion_config_json).await
        .map_err(|e| format!("Failed to send motion config: {}", e))?;
    
    // Step 4: Send hardware config frames to device
    // This includes all hardware configurations from hardware.json:
    // - Motor config (step_pin, dir_pin, enable_pin, steps_per_mm, etc.)
    // - Temperature config (hotbed, hotend sensors and PID parameters)
    // - Heater config (hotbed, hotend heaters and safety parameters)
    // - GPIO config (output pins like box_fan, chamber_led; input pins like filament_sensor, door_sensor)
    // - Limit switch config (if configured)
    // - Fan config (if configured)
    let printer_config = build_printer_config(&configs);
    let config_frames = ConfigFrameBuilder::build_config_frames(&printer_config);
    
    for frame_bytes in config_frames.iter() {
        client.serial_send_raw(frame_bytes).await
            .map_err(|e| format!("Failed to send config frame: {}", e))?;
        // Small delay between frames
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    
    // Step 5: Send ConfigComplete to notify device that all configs are sent
    client.serial_config_complete().await
        .map_err(|e| format!("Failed to send ConfigComplete: {}", e))?;
    
    Ok(())
}


