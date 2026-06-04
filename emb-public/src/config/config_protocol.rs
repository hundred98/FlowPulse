use super::printer_config::{PrinterJsonConfig, MotorParams, LimitSwitchAxis, TempSensorParams, HeaterPin, FanParams, LimitSwitchParams};
use crate::common::pin_parser::parse_pin;

pub const FRAME_SOF: u8 = 0xAA;
pub const FRAME_EOF: u8 = 0x55;
pub const FRAME_TYPE_CONFIG: u8 = 0x05;

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


        frames
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
        Self::build_temp_frame(0x04, temp)
    }

    fn build_temp_hotend_frame(temp: &TempSensorParams) -> Vec<u8> {
        Self::build_temp_frame(0x05, temp)
    }

    fn build_temp_frame(subtype: u8, temp: &TempSensorParams) -> Vec<u8> {
        let mut payload = vec![subtype];

        let adc = parse_pin(&temp.adc_pin);
        payload.push(adc.map(|p| p.port).unwrap_or(2));
        payload.push(adc.map(|p| p.pin).unwrap_or(0));

        let beta_bytes = temp.beta.to_le_bytes();
        payload.extend_from_slice(&beta_bytes[..2]);

        let r25_bytes = temp.r25.to_le_bytes();
        payload.extend_from_slice(&r25_bytes[..4]);

        let r_series_bytes = temp.r_series.to_le_bytes();
        payload.extend_from_slice(&r_series_bytes[..4]);

        let kp_bytes = temp.kp.to_le_bytes();
        let ki_bytes = temp.ki.to_le_bytes();
        let kd_bytes = temp.kd.to_le_bytes();
        payload.extend_from_slice(&kp_bytes[..4]);
        payload.extend_from_slice(&ki_bytes[..4]);
        payload.extend_from_slice(&kd_bytes[..4]);

        payload.push((temp.pid_interval_ms >> 0) as u8);
        payload.push((temp.pid_interval_ms >> 8) as u8);

        Self::wrap_frame(FRAME_TYPE_CONFIG, &payload)
    }

    fn build_heater_hotbed_frame(heater: &HeaterPin) -> Vec<u8> {
        Self::build_heater_frame(0x06, heater)
    }

    fn build_heater_hotend_frame(heater: &HeaterPin) -> Vec<u8> {
        Self::build_heater_frame(0x07, heater)
    }

    fn build_heater_frame(subtype: u8, heater: &HeaterPin) -> Vec<u8> {
        let mut payload = vec![subtype];

        let pin = parse_pin(&heater.pin);
        payload.push(pin.map(|p| p.port).unwrap_or(0xFF));
        payload.push(pin.map(|p| p.pin).unwrap_or(0xFF));
        payload.push(if heater.active_high { 1 } else { 0 });

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