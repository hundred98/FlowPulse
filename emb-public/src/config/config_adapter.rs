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
use crate::CoreSocketClient;
use super::config_protocol::ConfigFrameBuilder;

// ── hardware.json structures ──────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HardwareConfig {
    pub communication: Option<CommunicationConfig>,
    pub motor: Vec<MotorConfig>,
    pub gpio: Option<GpioConfig>,
    pub temperature: Option<TemperatureHardwareConfig>,
    pub heater: Option<HeaterHardwareConfig>,
    pub fan: Option<Vec<FanHardwareConfig>>,
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
    pub position_min: f32,
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

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct VelocityProfileFile {
    #[serde(default)]
    #[allow(dead_code)]
    pub r#type: String,
    pub six_point: Option<SixPointFile>,
}

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

// ── printer.json structures ───────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PrinterFileConfig {
    #[allow(dead_code)]
    pub version: String,
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

/// Read and parse all 3 config files from the given directory.
pub fn load_configs(config_dir: &str) -> Result<LoadedConfigs, String> {
    let hw_path = format!("{}/hardware.json", config_dir);
    let mo_path = format!("{}/motion.json", config_dir);
    let pr_path = format!("{}/printer.json", config_dir);

    let hw_str = std::fs::read_to_string(&hw_path)
        .map_err(|e| format!("Failed to read {}: {}", hw_path, e))?;
    let mo_str = std::fs::read_to_string(&mo_path)
        .map_err(|e| format!("Failed to read {}: {}", mo_path, e))?;
    let pr_str = std::fs::read_to_string(&pr_path)
        .map_err(|e| format!("Failed to read {}: {}", pr_path, e))?;

    let hardware: HardwareConfig = serde_json::from_str(&hw_str)
        .map_err(|e| format!("Parse {} error: {}", hw_path, e))?;
    let motion: MotionFileConfig = serde_json::from_str(&mo_str)
        .map_err(|e| format!("Parse {} error: {}", mo_path, e))?;
    let printer: PrinterFileConfig = serde_json::from_str(&pr_str)
        .map_err(|e| format!("Parse {} error: {}", pr_path, e))?;

    Ok(LoadedConfigs { hardware, motion, printer })
}

/// All loaded configuration data.
#[derive(Clone)]
#[allow(dead_code)]
pub struct LoadedConfigs {
    pub hardware: HardwareConfig,
    pub motion: MotionFileConfig,
    pub printer: PrinterFileConfig,
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
        ..Default::default()
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
            log::info!("🔌 连接串口: {} @ {}", serial.port, serial.baud_rate);
            match client.serial_connect(&serial.port, serial.baud_rate).await {
                Ok(()) => log::info!("✅ 串口连接成功"),
                Err(e) => {
                    log::error!("❌ 串口连接失败: {}", e);
                    log::info!("💡 请确认下位机已连接到 {}", serial.port);
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
