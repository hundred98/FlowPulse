//! M-command converter
//!
//! This module converts M-commands to device control commands.

use emb_api::{GpioRequest, MCommand};
use crate::config::PrinterJsonConfig;

/// M指令转换错误
#[derive(Debug, Clone)]
pub enum ConvertError {
    /// 无效的风扇索引
    InvalidFanIndex(u8),
    /// 无效的加热器ID
    InvalidHeaterId(u8),
    /// 配置缺失
    MissingConfig(String),
}

/// 设备控制命令
#[derive(Debug, Clone)]
pub enum DeviceCommand {
    /// GPIO控制（通用）
    GpioControl {
        name: String,
        value: f32,
    },
    /// 温度设置
    SetTemperature {
        heater_id: u8,  // 0=热床, 1=热端
        temp: f32,
    },
    /// 运动参数更新（服务端处理，不下发）
    MotionParamUpdate,
}

/// M指令转换器
pub struct MCommandConverter {
    /// 打印机配置
    config: Option<PrinterJsonConfig>,
}

impl MCommandConverter {
    /// 创建新的转换器
    pub fn new() -> Self {
        Self { config: None }
    }

    /// 设置配置
    pub fn set_config(&mut self, config: PrinterJsonConfig) {
        self.config = Some(config);
    }

    /// 转换M指令为设备控制命令
    pub fn convert(&self, m_command: &MCommand) -> Result<Vec<DeviceCommand>, ConvertError> {
        match m_command {
            // === 温度控制 ===
            MCommand::SetHotendTemp { tool, temp } |
            MCommand::WaitHotendTemp { tool, temp } => {
                Ok(vec![DeviceCommand::SetTemperature {
                    heater_id: tool + 1,  // 1=热端
                    temp: *temp,
                }])
            }

            MCommand::SetBedTemp { temp } |
            MCommand::WaitBedTemp { temp } => {
                Ok(vec![DeviceCommand::SetTemperature {
                    heater_id: 0,  // 0=热床
                    temp: *temp,
                }])
            }

            // === 风扇控制 ===
            MCommand::SetFanSpeed { index, speed } => {
                let fan_name = self.get_fan_name(*index)?;
                let value = *speed as f32 / 255.0;  // 0-255 → 0.0-1.0
                Ok(vec![DeviceCommand::GpioControl {
                    name: fan_name,
                    value,
                }])
            }

            MCommand::FanOff { index } => {
                let fan_name = self.get_fan_name(*index)?;
                Ok(vec![DeviceCommand::GpioControl {
                    name: fan_name,
                    value: 0.0,
                }])
            }

            // === 运动参数（服务端处理）===
            MCommand::SetAcceleration { .. } |
            MCommand::SetMaxVelocity { .. } |
            MCommand::SetAccelParams { .. } |
            MCommand::SetStepsPerMm { .. } => {
                Ok(vec![DeviceCommand::MotionParamUpdate])
            }

            // === 其他指令（暂不转换）===
            MCommand::ExtruderAbsoluteMode |
            MCommand::ExtruderRelativeMode => {
                // 这些指令由客户端状态管理，不下发到下位机
                Ok(vec![])
            }
        }
    }

    /// 获取风扇名称
    fn get_fan_name(&self, index: u8) -> Result<String, ConvertError> {
        use crate::config::ConfigManager;

        let config = ConfigManager::instance().get_config()
            .map_err(|e| ConvertError::MissingConfig(e))?;

        config.fan.get(index as usize)
            .map(|fan| fan.name.clone())
            .ok_or(ConvertError::InvalidFanIndex(index))
    }

    /// 转换为GPIO请求
    pub fn to_gpio_request(cmd: &DeviceCommand) -> Option<GpioRequest> {
        match cmd {
            DeviceCommand::GpioControl { name, value } => {
                Some(GpioRequest::SetPin {
                    name: name.clone(),
                    value: *value,
                })
            }
            _ => None,
        }
    }
}

impl Default for MCommandConverter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_set_hotend_temp() {
        let converter = MCommandConverter::new();
        let cmd = MCommand::SetHotendTemp { tool: 0, temp: 200.0 };
        let result = converter.convert(&cmd).unwrap();

        assert_eq!(result.len(), 1);
        if let DeviceCommand::SetTemperature { heater_id, temp } = &result[0] {
            assert_eq!(*heater_id, 1);
            assert_eq!(*temp, 200.0);
        } else {
            panic!("Expected SetTemperature");
        }
    }

    #[test]
    fn test_convert_set_bed_temp() {
        let converter = MCommandConverter::new();
        let cmd = MCommand::SetBedTemp { temp: 60.0 };
        let result = converter.convert(&cmd).unwrap();

        assert_eq!(result.len(), 1);
        if let DeviceCommand::SetTemperature { heater_id, temp } = &result[0] {
            assert_eq!(*heater_id, 0);
            assert_eq!(*temp, 60.0);
        } else {
            panic!("Expected SetTemperature");
        }
    }

    #[test]
    fn test_convert_fan_speed() {
        let converter = MCommandConverter::new();
        let cmd = MCommand::SetFanSpeed { index: 0, speed: 128 };
        let result = converter.convert(&cmd).unwrap();

        assert_eq!(result.len(), 1);
        if let DeviceCommand::GpioControl { name, value } = &result[0] {
            assert_eq!(name, "box_fan");
            assert!((value - 0.502).abs() < 0.01);  // 128/255 ≈ 0.502
        } else {
            panic!("Expected GpioControl");
        }
    }

    #[test]
    fn test_convert_fan_off() {
        let converter = MCommandConverter::new();
        let cmd = MCommand::FanOff { index: 0 };
        let result = converter.convert(&cmd).unwrap();

        assert_eq!(result.len(), 1);
        if let DeviceCommand::GpioControl { name, value } = &result[0] {
            assert_eq!(name, "box_fan");
            assert_eq!(*value, 0.0);
        } else {
            panic!("Expected GpioControl");
        }
    }

    #[test]
    fn test_convert_motion_param() {
        let converter = MCommandConverter::new();
        let cmd = MCommand::SetMaxVelocity {
            x: Some(100.0),
            y: Some(100.0),
            z: Some(5.0),
            e: Some(50.0),
        };
        let result = converter.convert(&cmd).unwrap();

        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], DeviceCommand::MotionParamUpdate));
    }
}
