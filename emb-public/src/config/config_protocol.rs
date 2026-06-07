use super::printer_config::{PrinterJsonConfig, MotorParams, LimitSwitchAxis, TempSensorParams, HeaterPin, FanParams, LimitSwitchParams, OutputPinParams, InputPinParams};
use crate::common::pin_parser::parse_pin;

pub const FRAME_SOF: u8 = 0xAA;
pub const FRAME_EOF: u8 = 0x55;
pub const FRAME_TYPE_CONFIG: u8 = 0x05;
pub const FRAME_TYPE_SET_TEMP: u8 = 0x24;  // 设置目标温度（避免与服务端ConfigComplete=0x11冲突）
pub const FRAME_TYPE_STATUS_R: u8 = 0x04;  // 状态响应（包含温度）

// Config frame subtypes - must match STM32 firmware definitions (emb_protocol.h)
pub const CONFIG_SUBTYPE_MOTOR: u8 = 0x01;
pub const CONFIG_SUBTYPE_TEMP: u8 = 0x02;
pub const CONFIG_SUBTYPE_LIMIT_SWITCH: u8 = 0x03;
pub const CONFIG_SUBTYPE_MOTION: u8 = 0x04;
pub const CONFIG_SUBTYPE_SYSTEM: u8 = 0x05;
pub const CONFIG_SUBTYPE_GPIO: u8 = 0x06;
pub const CONFIG_SUBTYPE_GPIO_OUTPUT: u8 = 0x07;  // Matches CONFIG_SUB_GPIO_OUTPUT on STM32
pub const CONFIG_SUBTYPE_GPIO_INPUT: u8 = 0x08;   // Matches CONFIG_SUB_GPIO_INPUT on STM32
pub const CONFIG_SUBTYPE_QUERY: u8 = 0x10;

// GPIO constants
pub const GPIO_TYPE_DIGITAL: u8 = 0;
pub const GPIO_TYPE_PWM: u8 = 1;
pub const GPIO_TYPE_ANALOG: u8 = 2;

pub const GPIO_PULL_NONE: u8 = 0;
pub const GPIO_PULL_UP: u8 = 1;
pub const GPIO_PULL_DOWN: u8 = 2;

pub const GPIO_EVENT_NONE: u8 = 0;
pub const GPIO_EVENT_FILAMENT_RUNOUT: u8 = 1;
pub const GPIO_EVENT_POWER_LOSS: u8 = 2;
pub const GPIO_EVENT_CUSTOM: u8 = 3;

pub const GPIO_REPORT_MODE_ON_CHANGE: u8 = 0;
pub const GPIO_REPORT_MODE_INTERVAL: u8 = 1;
pub const GPIO_REPORT_MODE_NONE: u8 = 2;

pub const GPIO_TRIGGER_RISING: u8 = 0;
pub const GPIO_TRIGGER_FALLING: u8 = 1;
pub const GPIO_TRIGGER_BOTH: u8 = 2;

pub struct ConfigFrameBuilder {
    #[allow(dead_code)]
    buffer: Vec<u8>,
}

impl ConfigFrameBuilder {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn build_config_frames(config: &PrinterJsonConfig) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();

        if !config.motor.is_empty() {
            frames.push(Self::build_motor_frame(&config.motor));
        }

        if config.limit_switch.x.pin != "PA0" || config.limit_switch.y.pin != "PA0" || config.limit_switch.z.pin != "PA0" {
            let limit_frame = Self::build_limit_switch_frame(&config.limit_switch);
            frames.push(limit_frame);
        }

        if !config.temperature.hotbed.adc_pin.is_empty() {
            frames.push(Self::build_temp_hotbed_frame(&config.temperature.hotbed));
        }
        if !config.temperature.hotend.adc_pin.is_empty() {
            frames.push(Self::build_temp_hotend_frame(&config.temperature.hotend));
        }

        if !config.heater.hotbed.pin.is_empty() {
            frames.push(Self::build_heater_hotbed_frame(&config.heater.hotbed));
        }
        if !config.heater.hotend.pin.is_empty() {
            frames.push(Self::build_heater_hotend_frame(&config.heater.hotend));
        }

        for fan in &config.fan {
            if !fan.pin.is_empty() {
                frames.push(Self::build_fan_frame(fan));
            }
        }

        // GPIO output pins
        for pin in &config.gpio.output {
            if !pin.pin.is_empty() {
                frames.extend(Self::build_gpio_output_frames(pin));
            }
        }

        // GPIO input pins
        for pin in &config.gpio.input {
            if !pin.pin.is_empty() {
                frames.extend(Self::build_gpio_input_frames(pin));
            }
        }

        frames
    }

    /// 构建状态查询帧（StatusQuery，帧类型 0x03）
    /// 发送此帧后，下位机会:
    /// 1. 立即回复 DeviceStatusReport
    /// 2. 启动定时上报（periodic_report_enabled = 1）
    pub fn build_status_query_frame() -> Vec<u8> {
        // StatusQuery 帧无 payload，仅 TYPE=0x03
        Self::wrap_frame(0x03, &[])
    }

    /// 构建设置温度帧
    /// heater_id: 0=热床, 1=热端
    /// target_temp: 目标温度（摄氏度）
    pub fn build_set_temp_frame(heater_id: u8, target_temp: f32) -> Vec<u8> {
        let mut payload = vec![heater_id];

        let temp_bytes = target_temp.to_be_bytes();
        payload.extend_from_slice(&temp_bytes[..4]);

        Self::wrap_frame(FRAME_TYPE_SET_TEMP, &payload)
    }

    fn build_motor_frame(motors: &[MotorParams]) -> Vec<u8> {
        let mut payload = vec![0x01];

        for motor in motors {
            let step = parse_pin(&motor.step_pin);
            let dir = parse_pin(&motor.dir_pin);
            let enable = parse_pin(&motor.enable_pin);
            let uart = parse_pin(&motor.driver.uart_pin);

            payload.push(motor.axis.as_bytes().first().copied().unwrap_or(b'X'));

            payload.push(step.map(|p| p.port).unwrap_or(0));
            payload.push(step.map(|p| p.pin).unwrap_or(0));
            payload.push(dir.map(|p| p.port).unwrap_or(0));
            payload.push(dir.map(|p| p.pin).unwrap_or(0));
            payload.push(if dir.map(|p| p.inverted).unwrap_or(false) { 1 } else { 0 });
            payload.push(enable.map(|p| p.port).unwrap_or(0));
            payload.push(enable.map(|p| p.pin).unwrap_or(0));
            payload.push(if enable.map(|p| p.inverted).unwrap_or(false) { 1 } else { 0 });
            payload.push(uart.map(|p| p.port).unwrap_or(0));
            payload.push(uart.map(|p| p.pin).unwrap_or(0));
        }

        Self::wrap_frame(FRAME_TYPE_CONFIG, &payload)
    }

    fn build_limit_switch_frame(limit: &LimitSwitchParams) -> Vec<u8> {
        let mut payload = vec![0x03];

        payload.extend_from_slice(&Self::limit_axis_to_bytes(&limit.x));
        payload.extend_from_slice(&Self::limit_axis_to_bytes(&limit.y));
        payload.extend_from_slice(&Self::limit_axis_to_bytes(&limit.z));

        payload.push((limit.homing_speed_mm_per_s >> 0) as u8);
        payload.push((limit.homing_speed_mm_per_s >> 8) as u8);
        payload.push(limit.homing_dir_x);
        payload.push(limit.homing_dir_y);
        payload.push(limit.homing_dir_z);

        Self::wrap_frame(FRAME_TYPE_CONFIG, &payload)
    }

    fn limit_axis_to_bytes(axis: &LimitSwitchAxis) -> [u8; 4] {
        let pin = parse_pin(&axis.pin);
        let port = pin.map(|p| p.port).unwrap_or(0xFF);
        let pin_num = pin.map(|p| p.pin).unwrap_or(0xFF);
        let inverted = pin.map(|p| p.inverted).unwrap_or(false);
        let pull = match axis.pull.as_str() {
            "up" => 0x01,
            "down" => 0x02,
            _ => 0x00,
        };
        let active_high = if axis.active_high { 1 } else { 0 };

        [
            (port << 1) | (inverted as u8),
            pin_num,
            (pull << 2) | (active_high << 1) | 0,
            0,
        ]
    }

    fn build_temp_hotbed_frame(temp: &TempSensorParams) -> Vec<u8> {
        Self::build_temp_frame(0x20, 0, temp)  // CONFIG_SUB_TEMP_SENSOR, index=0 (热床)
    }

    fn build_temp_hotend_frame(temp: &TempSensorParams) -> Vec<u8> {
        Self::build_temp_frame(0x20, 1, temp)  // CONFIG_SUB_TEMP_SENSOR, index=1 (热端)
    }

    fn build_temp_frame(subtype: u8, index: u8, temp: &TempSensorParams) -> Vec<u8> {
        let mut payload = vec![subtype, index];  // 添加索引字段

        let adc = parse_pin(&temp.adc_pin);
        payload.push(adc.map(|p| p.port).unwrap_or(2));
        payload.push(adc.map(|p| p.pin).unwrap_or(0));

        // beta 定义为 u32 但协议仅传输 2 字节（固件读 uint16_t），
        // 必须转为 u16 后再大端序列化，避免取到高 2 字节的 0
        let beta_bytes = (temp.beta as u16).to_be_bytes();
        payload.extend_from_slice(&beta_bytes[..2]);

        let r25_bytes = temp.ntc_resistance_25c.to_be_bytes();
        payload.extend_from_slice(&r25_bytes[..4]);

        let pullup_bytes = temp.pullup_resistor.to_be_bytes();
        payload.extend_from_slice(&pullup_bytes[..4]);

        let kp_bytes = temp.kp.to_be_bytes();
        let ki_bytes = temp.ki.to_be_bytes();
        let kd_bytes = temp.kd.to_be_bytes();
        payload.extend_from_slice(&kp_bytes[..4]);
        payload.extend_from_slice(&ki_bytes[..4]);
        payload.extend_from_slice(&kd_bytes[..4]);

        let pid_bytes = temp.pid_interval_ms.to_be_bytes();
        payload.extend_from_slice(&pid_bytes[..2]);

        // 添加安全限制参数
        let min_temp_bytes = temp.min_temp.to_be_bytes();
        payload.extend_from_slice(&min_temp_bytes[..2]);

        let max_temp_bytes = temp.max_temp.to_be_bytes();
        payload.extend_from_slice(&max_temp_bytes[..2]);

        Self::wrap_frame(FRAME_TYPE_CONFIG, &payload)
    }

    fn build_heater_hotbed_frame(heater: &HeaterPin) -> Vec<u8> {
        Self::build_heater_frame(0x21, 0, heater)  // CONFIG_SUB_HEATER, index=0 (热床)
    }

    fn build_heater_hotend_frame(heater: &HeaterPin) -> Vec<u8> {
        Self::build_heater_frame(0x21, 1, heater)  // CONFIG_SUB_HEATER, index=1 (热端)
    }

    fn build_heater_frame(subtype: u8, index: u8, heater: &HeaterPin) -> Vec<u8> {
        let mut payload = vec![subtype, index];  // 添加索引字段

        let pin = parse_pin(&heater.pin);
        payload.push(pin.map(|p| p.port).unwrap_or(0xFF));
        payload.push(pin.map(|p| p.pin).unwrap_or(0xFF));
        payload.push(if heater.active_high { 1 } else { 0 });

        // 添加PWM频率和最大功率
        let pwm_freq_bytes = heater.pwm_freq_hz.to_be_bytes();
        payload.extend_from_slice(&pwm_freq_bytes[..2]);
        payload.push(heater.max_power);

        // 添加安全配置
        let max_temp_dev_bytes = heater.safety.max_temp_deviation.to_be_bytes();
        payload.extend_from_slice(&max_temp_dev_bytes[..2]);

        let min_temp_dev_bytes = heater.safety.min_temp_deviation.to_be_bytes();
        payload.extend_from_slice(&min_temp_dev_bytes[..2]);

        let heating_timeout_bytes = heater.safety.heating_timeout_ms.to_be_bytes();
        payload.extend_from_slice(&heating_timeout_bytes[..4]);

        let sensor_fault_bytes = heater.safety.sensor_fault_threshold.to_be_bytes();
        payload.extend_from_slice(&sensor_fault_bytes[..2]);

        Self::wrap_frame(FRAME_TYPE_CONFIG, &payload)
    }

    fn build_fan_frame(fan: &FanParams) -> Vec<u8> {
        let mut payload = vec![0x08];

        payload.push(fan.name.as_bytes().first().copied().unwrap_or(b'F'));

        let pin = parse_pin(&fan.pin);
        payload.push(pin.map(|p| p.port).unwrap_or(0xFF));
        payload.push(pin.map(|p| p.pin).unwrap_or(0xFF));
        payload.push(if fan.active_high { 1 } else { 0 });

        let freq_bytes = fan.pwm_freq_hz.to_le_bytes();
        payload.extend_from_slice(&freq_bytes[..2]);

        Self::wrap_frame(FRAME_TYPE_CONFIG, &payload)
    }

    fn build_gpio_output_frames(pin: &OutputPinParams) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        let mut buf = Vec::new();

        buf.push(CONFIG_SUBTYPE_GPIO_OUTPUT);
        buf.push(0);  // pin_count = 0 表示追加模式

        let parsed_pin = match parse_pin(&pin.pin) {
            Some(p) => p,
            None => return frames,
        };

        let pin_type = match pin.pin_type {
            super::printer_config::OutputPinType::Pwm => GPIO_TYPE_PWM,
            super::printer_config::OutputPinType::Digital => GPIO_TYPE_DIGITAL,
        };

        let effective_active_high = parsed_pin.inverted ^ pin.active_high;

        let name_bytes = pin.name.as_bytes();
        let mut name_buf = [0u8; 16];
        let copy_len = name_bytes.len().min(16);
        name_buf[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        buf.extend_from_slice(&name_buf);

        buf.push(parsed_pin.port);
        buf.push(parsed_pin.pin);
        buf.push(pin_type);
        buf.push(if effective_active_high { 1 } else { 0 });

        buf.extend_from_slice(&pin.pwm_freq_hz.to_le_bytes());
        buf.extend_from_slice(&pin.default_value.to_be_bytes());
        buf.extend_from_slice(&pin.shutdown_value.to_be_bytes());
        buf.extend_from_slice(&pin.max_value.to_be_bytes());

        buf.push(0);
        buf.push(0);

        frames.push(Self::wrap_frame(FRAME_TYPE_CONFIG, &buf));
        frames
    }

    fn build_gpio_input_frames(pin: &InputPinParams) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        let mut buf = Vec::new();

        buf.push(CONFIG_SUBTYPE_GPIO_INPUT);
        buf.push(0);  // pin_count = 0 表示追加模式

        let parsed_pin = match parse_pin(&pin.pin) {
            Some(p) => p,
            None => return frames,
        };

        let pin_type = match pin.pin_type {
            super::printer_config::InputPinType::Digital => GPIO_TYPE_DIGITAL,
            super::printer_config::InputPinType::Analog => GPIO_TYPE_ANALOG,
        };

        let pull = match pin.pull.to_lowercase().as_str() {
            "up" => GPIO_PULL_UP,
            "down" => GPIO_PULL_DOWN,
            _ => GPIO_PULL_NONE,
        };

        let effective_active_high = parsed_pin.inverted ^ pin.active_high;

        let name_bytes = pin.name.as_bytes();
        let mut name_buf = [0u8; 16];
        let copy_len = name_bytes.len().min(16);
        name_buf[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        buf.extend_from_slice(&name_buf);

        buf.push(parsed_pin.port);
        buf.push(parsed_pin.pin);
        buf.push(pin_type);
        buf.push(pull);
        buf.push(if effective_active_high { 1 } else { 0 });

        buf.extend_from_slice(&pin.debounce_ms.to_le_bytes());

        let (event_action, report_mode, report_trigger, report_interval_ms, report_threshold) = 
            if let Some(ref report) = pin.report {
                let mode = match report.mode.to_lowercase().as_str() {
                    "on_change" => GPIO_REPORT_MODE_ON_CHANGE,
                    "interval" => GPIO_REPORT_MODE_INTERVAL,
                    _ => GPIO_REPORT_MODE_NONE,
                };

                let trigger = report.trigger.as_ref()
                    .map(|t| match t.to_lowercase().as_str() {
                        "rising" => GPIO_TRIGGER_RISING,
                        "falling" => GPIO_TRIGGER_FALLING,
                        "both" => GPIO_TRIGGER_BOTH,
                        _ => GPIO_TRIGGER_RISING,
                    })
                    .unwrap_or(GPIO_TRIGGER_RISING);

                let interval = report.interval_ms.unwrap_or(0) as u16;
                let threshold = report.threshold.unwrap_or(0.01);

                let event = if let Some(ref event) = pin.event {
                    match event.action.to_lowercase().as_str() {
                        "filament_runout" => GPIO_EVENT_FILAMENT_RUNOUT,
                        "power_loss" => GPIO_EVENT_POWER_LOSS,
                        "custom" => GPIO_EVENT_CUSTOM,
                        _ => GPIO_EVENT_NONE,
                    }
                } else {
                    GPIO_EVENT_NONE
                };

                (event, mode, trigger, interval, threshold)
            } else {
                (GPIO_EVENT_NONE, GPIO_REPORT_MODE_NONE, GPIO_TRIGGER_RISING, 0u16, 0.0f32)
            };

        buf.push(event_action);
        buf.push(report_mode);
        buf.push(report_trigger);
        buf.extend_from_slice(&report_interval_ms.to_le_bytes());
        buf.extend_from_slice(&report_threshold.to_be_bytes());

        let (cal_offset, cal_scale, cal_min, cal_max) = 
            if let Some(ref cal) = pin.calibration {
                (cal.offset, cal.scale, cal.min_value, cal.max_value)
            } else {
                (0.0f32, 1.0f32, 0.0f32, 1.0f32)
            };

        buf.extend_from_slice(&cal_offset.to_be_bytes());
        buf.extend_from_slice(&cal_scale.to_be_bytes());
        buf.extend_from_slice(&cal_min.to_be_bytes());
        buf.extend_from_slice(&cal_max.to_be_bytes());

        buf.push(pin.adc_resolution);

        frames.push(Self::wrap_frame(FRAME_TYPE_CONFIG, &buf));
        frames
    }

    fn wrap_frame(frame_type: u8, payload: &[u8]) -> Vec<u8> {
        let len = (payload.len() + 1) as u8;
        let mut frame = Vec::with_capacity(payload.len() + 6);

        frame.push(FRAME_SOF);
        frame.push(len);
        frame.push(frame_type);
        frame.extend_from_slice(payload);

        let crc = Self::crc8(&frame[1..]);
        frame.push(crc);
        frame.push(FRAME_EOF);

        frame
    }

    fn crc8(data: &[u8]) -> u8 {
        let mut crc = 0u8;
        for byte in data {
            crc ^= byte;
            for _ in 0..8 {
                if (crc & 0x80) != 0 {
                    crc = (crc << 1) ^ 0x31;
                } else {
                    crc <<= 1;
                }
            }
        }
        crc
    }
}

impl Default for ConfigFrameBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub fn create_config_frames(config: &PrinterJsonConfig) -> Vec<Vec<u8>> {
    ConfigFrameBuilder::build_config_frames(config)
}

pub fn validate_config(config: &PrinterJsonConfig) -> Result<(), String> {
    if config.motor.is_empty() {
        return Err("At least one motor must be configured".to_string());
    }

    for (i, motor) in config.motor.iter().enumerate() {
        if parse_pin(&motor.step_pin).is_none() {
            return Err(format!("Motor {} has invalid step_pin: {}", i, motor.step_pin));
        }
        if parse_pin(&motor.dir_pin).is_none() {
            return Err(format!("Motor {} has invalid dir_pin: {}", i, motor.dir_pin));
        }
        if parse_pin(&motor.enable_pin).is_none() {
            return Err(format!("Motor {} has invalid enable_pin: {}", i, motor.enable_pin));
        }
        if !motor.driver.uart_pin.is_empty() && parse_pin(&motor.driver.uart_pin).is_none() {
            return Err(format!("Motor {} has invalid driver.uart_pin: {}", i, motor.driver.uart_pin));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc8() {
        let data = [0x04, 0x02, 0x00, 0x96, 0x00];
        let crc = ConfigFrameBuilder::crc8(&data);
        assert_eq!(crc, 15);
    }

    #[test]
    fn test_parse_pin_in_config() {
        let pin = parse_pin("!PE4").unwrap();
        assert_eq!(pin.port, 4);
        assert_eq!(pin.pin, 4);
        assert!(pin.inverted);

        let pin2 = parse_pin("PA0").unwrap();
        assert_eq!(pin2.port, 0);
        assert_eq!(pin2.pin, 0);
        assert!(!pin2.inverted);
    }
}