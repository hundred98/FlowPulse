use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterJsonConfig {
    pub version: String,
    pub printer_model: String,
    #[serde(default)]
    pub communication: CommunicationConfig,
    #[serde(default)]
    pub printer: PrinterParams,
    #[serde(default)]
    pub gcode_settings: GCodeSettings,
    pub motor: Vec<MotorParams>,
    #[serde(default)]
    pub limit_switch: LimitSwitchParams,
    #[serde(default)]
    pub temperature: TemperatureParams,
    #[serde(default)]
    pub heater: HeaterParams,
    #[serde(default)]
    pub fan: Vec<FanParams>,
    #[serde(default)]
    pub probe: ProbeParams,
    #[serde(default)]
    pub pinout: PinoutInfo,
    #[serde(default)]
    pub gpio: GpioConfig,
    #[serde(default)]
    pub temperature_presets: Vec<TemperaturePresetConfig>,
    #[serde(default)]
    pub temperature_safety: Option<TemperatureSafetyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommunicationConfig {
    #[serde(default)]
    pub serial: SerialPortConfig,
    /// 状态上报间隔（毫秒），默认1000ms
    #[serde(default = "default_status_report_interval_ms")]
    pub status_report_interval_ms: u32,
}

fn default_status_report_interval_ms() -> u32 { 1000 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialPortConfig {
    #[serde(default = "default_serial_port")]
    pub port: String,
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    #[serde(default)]
    pub parity: String,
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u32,
    #[serde(default)]
    pub flow_control: bool,
}

fn default_serial_port() -> String { "COM3".to_string() }
fn default_baud_rate() -> u32 { 57600 }
fn default_data_bits() -> u8 { 8 }
fn default_stop_bits() -> u8 { 1 }
fn default_timeout_ms() -> u32 { 1000 }

impl Default for SerialPortConfig {
    fn default() -> Self {
        Self {
            port: default_serial_port(),
            baud_rate: default_baud_rate(),
            data_bits: default_data_bits(),
            parity: "None".to_string(),
            stop_bits: default_stop_bits(),
            timeout_ms: default_timeout_ms(),
            flow_control: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GCodeSettings {
    #[serde(default = "default_batch_size")]
    pub motion_batch_size: u8,
}

fn default_batch_size() -> u8 { 4 }

impl Default for GCodeSettings {
    fn default() -> Self {
        Self {
            motion_batch_size: default_batch_size(),
        }
    }
}

impl Default for PrinterJsonConfig {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            printer_model: "Default".to_string(),
            communication: CommunicationConfig::default(),
            printer: PrinterParams::default(),
            gcode_settings: GCodeSettings::default(),
            motor: vec![],
            limit_switch: LimitSwitchParams::default(),
            temperature: TemperatureParams::default(),
            heater: HeaterParams::default(),
            fan: vec![],
            probe: ProbeParams::default(),
            pinout: PinoutInfo::default(),
            gpio: GpioConfig::default(),
            temperature_presets: vec![
                TemperaturePresetConfig::default(),
                TemperaturePresetConfig {
                    name: "ABS".to_string(),
                    hotend_temp: 240.0,
                    bed_temp: 100.0,
                    chamber_temp: Some(50.0),
                    fan_speed: 0,
                },
            ],
            temperature_safety: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterParams {
    pub max_velocity: f32,
    pub max_acceleration: f32,
    pub square_corner_velocity: f32,
    pub junction_deviation: f32,

    #[serde(default)]
    pub velocity_profile: VelocityProfileConfig,
}

impl Default for PrinterParams {
    fn default() -> Self {
        Self {
            max_velocity: 400.0,
            max_acceleration: 3000.0,
            square_corner_velocity: 5.0,
            junction_deviation: 0.05,
            velocity_profile: VelocityProfileConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum VelocityProfileConfig {
    Trapezoidal,
    #[serde(rename = "s_curve")]
    SCurve {
        #[serde(default)]
        s_curve: SCurveConfig,
    },
    #[serde(rename = "six_point")]
    SixPoint {
        #[serde(default)]
        six_point: SixPointConfig,
    },
}

impl Default for VelocityProfileConfig {
    fn default() -> Self {
        Self::Trapezoidal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SCurveConfig {
    #[serde(default = "default_s_curve_enabled")]
    pub enabled: bool,

    #[serde(default = "default_jerk")]
    pub max_jerk_mm_s3: f32,

    #[serde(default = "default_min_distance")]
    pub min_distance_mm: f32,

    #[serde(default)]
    pub axis_specific: Option<HashMap<String, AxisJerkConfig>>,
}

fn default_s_curve_enabled() -> bool { false }
fn default_jerk() -> f32 { 50000.0 }
fn default_min_distance() -> f32 { 0.5 }

impl Default for SCurveConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_jerk_mm_s3: default_jerk(),
            min_distance_mm: default_min_distance(),
            axis_specific: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxisJerkConfig {
    pub max_jerk: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SixPointConfig {
    #[serde(default = "default_start_accel")]
    pub start_accel_mm_s2: f32,
    #[serde(default = "default_max_accel")]
    pub max_accel_mm_s2: f32,
    #[serde(default = "default_final_decel")]
    pub final_decel_mm_s2: f32,
    #[serde(default = "default_max_decel")]
    pub max_decel_mm_s2: f32,
    #[serde(default = "default_start_speed")]
    pub start_speed_mm_s: f32,
    #[serde(default = "default_stop_speed")]
    pub stop_speed_mm_s: f32,
    #[serde(default = "default_break_speed")]
    pub break_speed_mm_s: f32,
    #[serde(default = "default_min_distance")]
    pub min_distance_mm: f32,
}

fn default_start_accel() -> f32 { 500.0 }
fn default_max_accel() -> f32 { 2000.0 }
fn default_final_decel() -> f32 { 500.0 }
fn default_max_decel() -> f32 { 2000.0 }
fn default_start_speed() -> f32 { 10.0 }
fn default_stop_speed() -> f32 { 10.0 }
fn default_break_speed() -> f32 { 50.0 }
fn default_max_value() -> f32 { 1.0 }
fn default_adc_resolution() -> u8 { 12 }

impl Default for SixPointConfig {
    fn default() -> Self {
        Self {
            start_accel_mm_s2: default_start_accel(),
            max_accel_mm_s2: default_max_accel(),
            final_decel_mm_s2: default_final_decel(),
            max_decel_mm_s2: default_max_decel(),
            start_speed_mm_s: default_start_speed(),
            stop_speed_mm_s: default_stop_speed(),
            break_speed_mm_s: default_break_speed(),
            min_distance_mm: default_min_distance(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverParams {
    #[serde(rename = "uart_pin")]
    #[serde(default)]
    pub uart_pin: String,
    #[serde(rename = "microsteps")]
    #[serde(default)]
    pub microsteps: u8,
    #[serde(rename = "current_ma")]
    #[serde(default)]
    pub current_ma: u16,
    #[serde(rename = "hold_current_ma")]
    #[serde(default)]
    pub hold_current_ma: u16,
    #[serde(rename = "stealthchop_threshold")]
    #[serde(default)]
    pub stealthchop_threshold: u32,
}

impl Default for DriverParams {
    fn default() -> Self {
        Self {
            uart_pin: String::new(),
            microsteps: 16,
            current_ma: 800,
            hold_current_ma: 500,
            stealthchop_threshold: 999999,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtruderParams {
    #[serde(rename = "nozzle_diameter_mm")]
    #[serde(default)]
    pub nozzle_diameter_mm: Option<f32>,
    #[serde(rename = "filament_diameter_mm")]
    #[serde(default)]
    pub filament_diameter_mm: Option<f32>,
    #[serde(rename = "max_flow_rate")]
    #[serde(default)]
    pub max_flow_rate: Option<f32>,
}

impl Default for ExtruderParams {
    fn default() -> Self {
        Self {
            nozzle_diameter_mm: None,
            filament_diameter_mm: None,
            max_flow_rate: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotorParams {
    pub axis: String,
    #[serde(rename = "step_pin")]
    pub step_pin: String,
    #[serde(rename = "dir_pin")]
    pub dir_pin: String,
    #[serde(rename = "enable_pin")]
    pub enable_pin: String,
    #[serde(rename = "max_speed_mm_per_s")]
    pub max_speed_mm_per_s: u16,
    #[serde(rename = "max_accel")]
    pub max_accel: u32,
    #[serde(rename = "steps_per_mm")]
    pub steps_per_mm: u32,
    #[serde(rename = "position_min")]
    #[serde(default)]
    pub position_min: i32,
    #[serde(rename = "position_max")]
    #[serde(default)]
    pub position_max: i32,
    #[serde(rename = "driver")]
    #[serde(default)]
    pub driver: DriverParams,
    #[serde(rename = "extruder")]
    #[serde(default)]
    pub extruder: ExtruderParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitSwitchParams {
    pub x: LimitSwitchAxis,
    pub y: LimitSwitchAxis,
    pub z: LimitSwitchAxis,
    #[serde(rename = "homing_speed_mm_per_s")]
    pub homing_speed_mm_per_s: u16,
    #[serde(rename = "homing_dir_x")]
    #[serde(default)]
    pub homing_dir_x: u8,
    #[serde(rename = "homing_dir_y")]
    #[serde(default)]
    pub homing_dir_y: u8,
    #[serde(rename = "homing_dir_z")]
    #[serde(default)]
    pub homing_dir_z: u8,
}

impl Default for LimitSwitchParams {
    fn default() -> Self {
        Self {
            x: LimitSwitchAxis::default(),
            y: LimitSwitchAxis::default(),
            z: LimitSwitchAxis::default(),
            homing_speed_mm_per_s: 25,
            homing_dir_x: 0,
            homing_dir_y: 0,
            homing_dir_z: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitSwitchAxis {
    pub pin: String,
    pub pull: String,
    #[serde(rename = "active_high")]
    #[serde(default)]
    pub active_high: bool,
    #[serde(rename = "position_endstop")]
    #[serde(default)]
    pub position_endstop: Option<f32>,
}

impl Default for LimitSwitchAxis {
    fn default() -> Self {
        Self {
            pin: "PA0".to_string(),
            pull: "up".to_string(),
            active_high: false,
            position_endstop: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureParams {
    pub hotbed: TempSensorParams,
    pub hotend: TempSensorParams,
}

impl Default for TemperatureParams {
    fn default() -> Self {
        Self {
            hotbed: TempSensorParams::default(),
            hotend: TempSensorParams::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempSensorParams {
    #[serde(rename = "sensor_type")]
    pub sensor_type: String,
    #[serde(rename = "adc_pin")]
    pub adc_pin: String,
    pub beta: u32,
    #[serde(rename = "pullup_resistor")]
    pub pullup_resistor: u32,  // 上拉电阻值（Ω）
    #[serde(default)]
    pub min_temp: i16,
    #[serde(rename = "max_temp")]
    pub max_temp: u16,
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
    #[serde(rename = "pid_interval_ms")]
    #[serde(default = "default_pid_interval")]
    pub pid_interval_ms: u16,
}

impl Default for TempSensorParams {
    fn default() -> Self {
        Self {
            sensor_type: "NTC100K".to_string(),
            adc_pin: "PA0".to_string(),
            beta: 3950,
            pullup_resistor: 4700,
            min_temp: -100,
            max_temp: 300,
            kp: 22.2,
            ki: 1.08,
            kd: 114.0,
            pid_interval_ms: 100,
        }
    }
}

fn default_pid_interval() -> u16 { 100 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaterParams {
    pub hotbed: HeaterPin,
    pub hotend: HeaterPin,
}

impl Default for HeaterParams {
    fn default() -> Self {
        Self {
            hotbed: HeaterPin::default(),
            hotend: HeaterPin::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaterPin {
    pub pin: String,
    #[serde(rename = "active_high")]
    #[serde(default = "default_active_high")]
    pub active_high: bool,
    #[serde(rename = "pwm_freq_hz", default = "default_heater_pwm_freq")]
    pub pwm_freq_hz: u16,
    #[serde(rename = "max_power", default = "default_max_power")]
    pub max_power: u8,
    #[serde(default)]
    pub safety: HeaterSafetyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaterSafetyConfig {
    #[serde(default = "default_max_temp_deviation")]
    pub max_temp_deviation: i16,  // 允许超过目标温度的最大偏差（°C）
    #[serde(default = "default_min_temp_deviation")]
    pub min_temp_deviation: i16,  // 允许低于目标温度的最大偏差（°C）
    #[serde(default = "default_heating_timeout_ms")]
    pub heating_timeout_ms: u32,  // 加热超时时间（毫秒）
    #[serde(default = "default_sensor_fault_threshold")]
    pub sensor_fault_threshold: u16,  // 传感器故障检测阈值（ADC读数）
}

impl Default for HeaterSafetyConfig {
    fn default() -> Self {
        Self {
            max_temp_deviation: default_max_temp_deviation(),
            min_temp_deviation: default_min_temp_deviation(),
            heating_timeout_ms: default_heating_timeout_ms(),
            sensor_fault_threshold: default_sensor_fault_threshold(),
        }
    }
}

fn default_heater_pwm_freq() -> u16 { 10 }  // 默认10Hz
fn default_max_power() -> u8 { 100 }  // 默认最大功率100%
fn default_max_temp_deviation() -> i16 { 5 }  // 默认允许超过目标温度5°C
fn default_min_temp_deviation() -> i16 { -10 }  // 默认允许低于目标温度10°C
fn default_heating_timeout_ms() -> u32 { 300000 }  // 默认加热超时5分钟
fn default_sensor_fault_threshold() -> u16 { 50 }  // 默认ADC故障阈值50（接近0或4095）

impl Default for HeaterPin {
    fn default() -> Self {
        Self {
            pin: "PA0".to_string(),
            active_high: true,
            pwm_freq_hz: default_heater_pwm_freq(),
            max_power: default_max_power(),
            safety: HeaterSafetyConfig::default(),
        }
    }
}

fn default_active_high() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanParams {
    pub name: String,
    pub pin: String,
    #[serde(rename = "active_high")]
    #[serde(default = "default_active_high")]
    pub active_high: bool,
    #[serde(rename = "pwm_freq_hz")]
    #[serde(default = "default_pwm_freq")]
    pub pwm_freq_hz: u16,
}

impl Default for FanParams {
    fn default() -> Self {
        Self {
            name: "Fan".to_string(),
            pin: "PA0".to_string(),
            active_high: true,
            pwm_freq_hz: 100,
        }
    }
}

fn default_pwm_freq() -> u16 { 100 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeParams {
    pub pin: String,
    pub pull: String,
    #[serde(rename = "active_high")]
    #[serde(default)]
    pub active_high: bool,
}

impl Default for ProbeParams {
    fn default() -> Self {
        Self {
            pin: "PA0".to_string(),
            pull: "up".to_string(),
            active_high: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinoutInfo {
    #[serde(rename = "emergency_stop")]
    #[serde(default)]
    pub emergency_stop: Option<String>,
    pub uart: Option<HashMap<String, String>>,
    pub usb: Option<HashMap<String, String>>,
    pub usart3: Option<HashMap<String, String>>,
}

impl Default for PinoutInfo {
    fn default() -> Self {
        Self {
            emergency_stop: None,
            uart: None,
            usb: None,
            usart3: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpioConfig {
    #[serde(default)]
    pub output: Vec<OutputPinParams>,
    #[serde(default)]
    pub input: Vec<InputPinParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputPinParams {
    pub name: String,
    pub pin: String,
    #[serde(rename = "type")]
    pub pin_type: OutputPinType,
    #[serde(default = "default_active_high")]
    pub active_high: bool,
    #[serde(default = "default_pwm_freq")]
    pub pwm_freq_hz: u16,
    #[serde(default)]
    pub default_value: f32,
    #[serde(default)]
    pub shutdown_value: f32,
    #[serde(default = "default_max_value")]
    pub max_value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputPinType {
    Pwm,
    Digital,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputPinParams {
    pub name: String,
    pub pin: String,
    #[serde(rename = "type")]
    pub pin_type: InputPinType,
    pub pull: String,
    #[serde(default = "default_active_high")]
    pub active_high: bool,
    #[serde(default)]
    pub debounce_ms: u16,
    #[serde(default)]
    pub event: Option<InputEventConfig>,
    #[serde(default)]
    pub report: Option<InputReportConfig>,
    #[serde(default)]
    pub calibration: Option<AnalogCalibrationConfig>,
    #[serde(default = "default_adc_resolution")]
    pub adc_resolution: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputPinType {
    Digital,
    Analog,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEventConfig {
    pub action: String,
    #[serde(default)]
    pub threshold_below: Option<f32>,
    #[serde(default)]
    pub threshold_above: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputReportConfig {
    pub mode: String,
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default)]
    pub threshold: Option<f32>,
    #[serde(default)]
    pub interval_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalogCalibrationConfig {
    #[serde(default)]
    pub offset: f32,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default)]
    pub min_value: f32,
    #[serde(default = "default_max_value")]
    pub max_value: f32,
}

fn default_scale() -> f32 { 1.0 }

/// Temperature preset configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperaturePresetConfig {
    /// Preset name (e.g., "PLA", "ABS", "PETG")
    pub name: String,
    /// Hotend temperature in Celsius
    pub hotend_temp: f32,
    /// Bed temperature in Celsius
    pub bed_temp: f32,
    /// Chamber temperature in Celsius (optional)
    #[serde(default)]
    pub chamber_temp: Option<f32>,
    /// Fan speed (0-255)
    #[serde(default)]
    pub fan_speed: u8,
}

impl Default for TemperaturePresetConfig {
    fn default() -> Self {
        Self {
            name: "PLA".to_string(),
            hotend_temp: 200.0,
            bed_temp: 60.0,
            chamber_temp: None,
            fan_speed: 255,
        }
    }
}

/// Temperature safety configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureSafetyConfig {
    /// Safety check interval in milliseconds
    #[serde(default = "default_safety_check_interval_ms")]
    pub safety_check_interval_ms: u64,

    /// Temperature change threshold for triggering events (°C)
    #[serde(default = "default_temp_change_threshold")]
    pub temp_change_threshold: f32,

    /// Per-heater safety configuration
    #[serde(default)]
    pub heaters: HashMap<String, TempHeaterSafetyConfig>,
}

fn default_safety_check_interval_ms() -> u64 { 1000 }
fn default_temp_change_threshold() -> f32 { 1.0 }

impl Default for TemperatureSafetyConfig {
    fn default() -> Self {
        Self {
            safety_check_interval_ms: default_safety_check_interval_ms(),
            temp_change_threshold: default_temp_change_threshold(),
            heaters: HashMap::new(),
        }
    }
}

/// Per-heater safety configuration for temperature management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempHeaterSafetyConfig {
    /// Sensor fault detection thresholds
    pub sensor_fault: SensorFaultConfig,

    /// Temperature deviation thresholds
    pub deviation_thresholds: DeviationThresholdsConfig,

    /// Heating delay in seconds (don't check low temp during this period)
    #[serde(default = "default_heating_delay_secs")]
    pub heating_delay_secs: u64,

    /// Actions for different temperature conditions
    pub actions: HeaterActionsConfig,
}

fn default_heating_delay_secs() -> u64 { 60 }

impl Default for TempHeaterSafetyConfig {
    fn default() -> Self {
        Self {
            sensor_fault: SensorFaultConfig::default(),
            deviation_thresholds: DeviationThresholdsConfig::default(),
            heating_delay_secs: default_heating_delay_secs(),
            actions: HeaterActionsConfig::default(),
        }
    }
}

/// Sensor fault detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorFaultConfig {
    /// Maximum temperature threshold (°C), above this is sensor fault
    #[serde(default = "default_sensor_max_temp")]
    pub max_temp: f32,

    /// Minimum temperature threshold (°C), below this is sensor fault
    #[serde(default = "default_sensor_min_temp")]
    pub min_temp: f32,
}

fn default_sensor_max_temp() -> f32 { 300.0 }
fn default_sensor_min_temp() -> f32 { -50.0 }

impl Default for SensorFaultConfig {
    fn default() -> Self {
        Self {
            max_temp: default_sensor_max_temp(),
            min_temp: default_sensor_min_temp(),
        }
    }
}

/// Temperature deviation thresholds configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviationThresholdsConfig {
    /// Warning level deviation (°C)
    #[serde(default = "default_deviation_warning")]
    pub warning: f32,

    /// Critical level deviation (°C)
    #[serde(default = "default_deviation_critical")]
    pub critical: f32,

    /// Emergency level deviation (°C)
    #[serde(default = "default_deviation_emergency")]
    pub emergency: f32,
}

fn default_deviation_warning() -> f32 { 10.0 }
fn default_deviation_critical() -> f32 { 15.0 }
fn default_deviation_emergency() -> f32 { 20.0 }

impl Default for DeviationThresholdsConfig {
    fn default() -> Self {
        Self {
            warning: default_deviation_warning(),
            critical: default_deviation_critical(),
            emergency: default_deviation_emergency(),
        }
    }
}

/// Heater actions configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaterActionsConfig {
    /// Actions for low temperature conditions
    pub low_temp: TemperatureActionsConfig,

    /// Actions for high temperature conditions
    pub high_temp: TemperatureActionsConfig,
}

impl Default for HeaterActionsConfig {
    fn default() -> Self {
        Self {
            low_temp: TemperatureActionsConfig::default(),
            high_temp: TemperatureActionsConfig::default(),
        }
    }
}

/// Temperature actions configuration for different levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureActionsConfig {
    /// Action for warning level
    #[serde(default = "default_warning_action")]
    pub warning: String,

    /// Action for critical level
    #[serde(default = "default_critical_action")]
    pub critical: String,

    /// Action for emergency level
    #[serde(default = "default_emergency_action")]
    pub emergency: String,
}

fn default_warning_action() -> String { "warn".to_string() }
fn default_critical_action() -> String { "pause_print".to_string() }
fn default_emergency_action() -> String { "emergency_stop".to_string() }

impl Default for TemperatureActionsConfig {
    fn default() -> Self {
        Self {
            warning: default_warning_action(),
            critical: default_critical_action(),
            emergency: default_emergency_action(),
        }
    }
}

pub fn parse_json_config(content: &str) -> Result<PrinterJsonConfig, String> {
    serde_json::from_str(content)
        .map_err(|e| format!("JSON parse error: {}", e))
}

pub fn load_config_from_file(path: &str) -> Result<PrinterJsonConfig, String> {
    std::fs::read_to_string(path)
        .map_err(|e| format!("File read error: {}", e))
        .and_then(|content| parse_json_config(&content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_printer_config() {
        let json = r#"{
            "version": "1.0",
            "printer_model": "STM32F407_3DPrinter",
            "printer": {
                "max_velocity": 300,
                "max_accel": 3000,
                "square_corner_velocity": 5.0
            },
            "motor": [
                {
                    "axis": "X",
                    "step_pin": "PE3",
                    "dir_pin": "PE2",
                    "enable_pin": "!PE4",
                    "max_speed_mm_per_s": 400,
                    "max_accel": 20000,
                    "steps_per_mm": 80,
                    "uart_pin": "",
                    "stealthchop_threshold": 999999
                }
            ],
            "limit_switch": {
                "x": { "pin": "PA15", "pull": "up", "active_high": false, "position_endstop": 0 },
                "y": { "pin": "!PD2", "pull": "up", "active_high": false, "position_endstop": 0 },
                "z": { "pin": "!PC8", "pull": "up", "active_high": false, "position_endstop": 0.5 },
                "homing_speed_mm_per_s": 20
            },
            "temperature": {
                "hotbed": {
                    "sensor_type": "EPCOS 100K B57560G104F",
                    "adc_pin": "PC0",
                    "beta": 3950,
                    "r25": 100000,
                    "r_series": 4700,
                    "target_temp": 0,
                    "min_temp": 0,
                    "max_temp": 130,
                    "kp": 325.10,
                    "ki": 63.35,
                    "kd": 417.10
                },
                "hotend": {
                    "sensor_type": "ATC Semitec 104GT-2",
                    "adc_pin": "PC1",
                    "beta": 3950,
                    "r25": 100000,
                    "r_series": 4700,
                    "target_temp": 0,
                    "min_temp": 0,
                    "max_temp": 250,
                    "kp": 14.669,
                    "ki": 0.572,
                    "kd": 94.068
                }
            },
            "heater": {
                "hotbed": { "pin": "PA0", "active_high": true },
                "hotend": { "pin": "PE5", "active_high": true }
            },
            "fan": [
                { "name": "FAN0", "pin": "PB1", "active_high": true, "pwm_freq_hz": 100 }
            ],
            "probe": { "pin": "NC", "pull": "up", "active_high": true }
        }"#;

        let config = parse_json_config(json).unwrap();
        assert_eq!(config.printer_model, "STM32F407_3DPrinter");
        assert_eq!(config.printer.max_velocity, 300);
        assert_eq!(config.motor[0].axis, "X");
        assert_eq!(config.motor[0].step_pin, "PE3");
    }
}