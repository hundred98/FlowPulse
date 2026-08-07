use super::printer_config::{PrinterJsonConfig, MotorParams, DriverParams, LimitSwitchAxis, TempSensorParams, HeaterPin, FanParams, LimitSwitchParams, OutputPinParams, InputPinParams, PidTuneHeaterConfig};
use crate::common::pin_parser::parse_pin;

pub const FRAME_SOF: u8 = 0xAA;
pub const FRAME_EOF: u8 = 0x55;
pub const FRAME_TYPE_CONFIG: u8 = 0x05;
pub const FRAME_TYPE_SET_TEMP: u8 = 0x24;  // 设置目标温度（避免与服务端ConfigComplete=0x11冲突）
pub const FRAME_TYPE_STATUS_R: u8 = 0x04;  // 状态响应（包含温度）
pub const FRAME_TYPE_TMC_CONFIG: u8 = 0x2A;  // TMC2209静态配置(Host→Device)，独立帧
pub const FRAME_TYPE_TMC_CONFIG_ACK: u8 = 0x2B;  // TMC2209配置响应(Device→Host)
pub const FRAME_TYPE_TMC_STALL_CFG: u8 = 0x28;  // TMC2209 StallGuard配置(Host→Device)
pub const FRAME_TYPE_TMC_STALL_ACK: u8 = 0x29;  // TMC2209 StallGuard确认(Device→Host)

pub const CONFIG_SUB_MCU2: u8 = 0x22;  // 转给 MCU2 的配置(Host→MCU1→MCU2)，MCU1 暂存不本地应用

// TMC 轴索引
pub const TMC_AXIS_X: u8 = 0;
pub const TMC_AXIS_Y: u8 = 1;
pub const TMC_AXIS_Z: u8 = 2;

// Config frame subtypes - must match STM32 firmware definitions (emb_protocol.h)
pub const CONFIG_SUBTYPE_MOTOR: u8 = 0x01;
pub const CONFIG_SUBTYPE_TEMP: u8 = 0x02;
pub const CONFIG_SUBTYPE_LIMIT_SWITCH: u8 = 0x03;
pub const CONFIG_SUBTYPE_MOTION: u8 = 0x04;
pub const CONFIG_SUBTYPE_SYSTEM: u8 = 0x05;
pub const CONFIG_SUBTYPE_GPIO: u8 = 0x06;
pub const CONFIG_SUBTYPE_GPIO_OUTPUT: u8 = 0x07;  // Matches CONFIG_SUB_GPIO_OUTPUT on STM32
pub const CONFIG_SUBTYPE_GPIO_INPUT: u8 = 0x08;   // Matches CONFIG_SUB_GPIO_INPUT on STM32
pub const CONFIG_SUBTYPE_PID_TUNE: u8 = 0x09;   // PID整定配置
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

        // 区分 MCU1 本机与 MCU2 转发的配置
        let mcu1_motors: Vec<&MotorParams> = config.motor.iter()
            .filter(|m| m.mcu.eq_ignore_ascii_case("MCU1"))
            .collect();
        let mcu2_motors: Vec<&MotorParams> = config.motor.iter()
            .filter(|m| m.mcu.eq_ignore_ascii_case("MCU2"))
            .collect();

        // MCU2 配置项收集：sub_type + data(不含 sub_type)
        let mut mcu2_items: Vec<(u8, Vec<u8>)> = Vec::new();

        // 电机配置帧：MCU1 轴走 CONFIG_SUB_MOTOR(0x01)，MCU2 轴打包进 CONFIG_SUB_MCU2(0x22)
        if !mcu1_motors.is_empty() {
            frames.push(Self::build_motor_frame(&mcu1_motors));
        }
        if !mcu2_motors.is_empty() {
            if let Some(item) = Self::subframe_to_item(&Self::build_motor_frame(&mcu2_motors)) {
                mcu2_items.push(item);
            }
        }

        // limit_switch 帧：只要任一轴配置了归位参数(speed/fine/retract/dir)或 limit 引脚就发送。
        // 兼容两种归位模式：
        //   - 机械归位轴: 配置了 limit 引脚，用 limit 触发
        //   - sensorless 轴: 不配 limit 引脚，但仍需要 homing_speed(PRE速度)/retract/方向，
        //     这些参数也必须下发，故不能仅按 pin 是否为空判断。
        let ls = &config.limit_switch;
        let has_limit_cfg = !ls.x.pin.is_empty() || !ls.y.pin.is_empty() || !ls.z.pin.is_empty()
            || ls.x.homing_speed_mm_per_s != 25 || ls.y.homing_speed_mm_per_s != 25
            || ls.z.homing_speed_mm_per_s != 25
            || ls.x.homing_retract_mm != 5.0 || ls.y.homing_retract_mm != 5.0
            || ls.z.homing_retract_mm != 5.0
            || ls.x.homing_dir != 0 || ls.y.homing_dir != 0 || ls.z.homing_dir != 0;
        if has_limit_cfg {
            let limit_frame = Self::build_limit_switch_frame(&config.limit_switch, &config.motor);
            frames.push(limit_frame);
        }

        // 温度传感器：按 mcu 归属区分
        if !config.temperature.hotbed.adc_pin.is_empty() {
            if config.temperature.hotbed.mcu.eq_ignore_ascii_case("MCU2") {
                if let Some(item) = Self::subframe_to_item(&Self::build_temp_hotbed_frame(&config.temperature.hotbed)) {
                    mcu2_items.push(item);
                }
            } else {
                frames.push(Self::build_temp_hotbed_frame(&config.temperature.hotbed));
            }
        }
        if !config.temperature.hotend.adc_pin.is_empty() {
            if config.temperature.hotend.mcu.eq_ignore_ascii_case("MCU2") {
                if let Some(item) = Self::subframe_to_item(&Self::build_temp_hotend_frame(&config.temperature.hotend)) {
                    mcu2_items.push(item);
                }
            } else {
                frames.push(Self::build_temp_hotend_frame(&config.temperature.hotend));
            }
        }

        // 加热器：按 mcu 归属区分
        if !config.heater.hotbed.pin.is_empty() {
            if config.heater.hotbed.mcu.eq_ignore_ascii_case("MCU2") {
                if let Some(item) = Self::subframe_to_item(&Self::build_heater_hotbed_frame(&config.heater.hotbed)) {
                    mcu2_items.push(item);
                }
            } else {
                frames.push(Self::build_heater_hotbed_frame(&config.heater.hotbed));
            }
        }
        if !config.heater.hotend.pin.is_empty() {
            if config.heater.hotend.mcu.eq_ignore_ascii_case("MCU2") {
                if let Some(item) = Self::subframe_to_item(&Self::build_heater_hotend_frame(&config.heater.hotend)) {
                    mcu2_items.push(item);
                }
            } else {
                frames.push(Self::build_heater_hotend_frame(&config.heater.hotend));
            }
        }

        for fan in &config.fan {
            if !fan.pin.is_empty() {
                frames.push(Self::build_fan_frame(fan));
            }
        }

        // GPIO output pins：MCU1 本机应用，MCU2 的打包进 CONFIG_SUB_MCU2(0x22)
        for pin in &config.gpio.output {
            if pin.pin.is_empty() { continue; }
            if pin.mcu.eq_ignore_ascii_case("MCU2") {
                for f in Self::build_gpio_output_frames(pin) {
                    if let Some(item) = Self::subframe_to_item(&f) {
                        mcu2_items.push(item);
                    }
                }
            } else {
                frames.extend(Self::build_gpio_output_frames(pin));
            }
        }

        // GPIO input pins
        for pin in &config.gpio.input {
            if pin.pin.is_empty() { continue; }
            if pin.mcu.eq_ignore_ascii_case("MCU2") {
                for f in Self::build_gpio_input_frames(pin) {
                    if let Some(item) = Self::subframe_to_item(&f) {
                        mcu2_items.push(item);
                    }
                }
            } else {
                frames.extend(Self::build_gpio_input_frames(pin));
            }
        }

        // System config (status report interval)
        frames.push(Self::build_system_frame(config.communication.status_report_interval_ms));

        // PID tune configuration (热端 + 热床)
        if let Some(ref pid_tune) = config.pid_tune {
            frames.push(Self::build_pid_tune_hotend_frame(&pid_tune.hotend));
            frames.push(Self::build_pid_tune_hotbed_frame(&pid_tune.hotbed));
        }

        // MCU2 配置统一打包进 CONFIG_SUB_MCU2(0x22) 独立帧，最后下发。
        if !mcu2_items.is_empty() {
            frames.push(Self::build_mcu2_config_frame(&mcu2_items));
        }

        // TMC2209 独立静态配置帧 (0x2A)。
        // 仅对归属于 MCU1 的轴生成（E 轴若归属 MCU2，由 MCU2 侧处理）。
        // 必须在所有 CONFIG 子帧设置完毕后下发，因为下位机执行串口配置耗时较长。
        // 注意：StallGuard 帧 (0x28) 不属于启动配置阶段，由归位流程单独触发。
        frames.extend(Self::build_tmc_config_frames(&config.motor));

        frames
    }

    /// 从完整的 CONFIG 子帧中提取 (sub_type, data_without_subtype)，用于 MCU2 打包。
    /// 完整帧格式: [SOF][len][type][payload][crc][EOF]，payload 从 index 3 到 len-2。
    fn subframe_to_item(frame: &[u8]) -> Option<(u8, Vec<u8>)> {
        if frame.len() < 6 { return None; }
        let payload = &frame[3..frame.len() - 2];
        if payload.is_empty() { return None; }
        Some((payload[0], payload[1..].to_vec()))
    }

    /// 构建 CONFIG_SUB_MCU2(0x22) 帧，把多个 MCU2 配置项打包为 TLV 序列。
    /// payload: [0x22][count][item...]，每个 item = [sub_type][sub_len][sub_data]。
    /// MCU1 收到后暂存，后续由 MCU1 转发给 MCU2。
    fn build_mcu2_config_frame(items: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.push(CONFIG_SUB_MCU2);
        payload.push(items.len() as u8);
        for (sub_type, data) in items {
            payload.push(*sub_type);
            payload.push(data.len() as u8);
            payload.extend_from_slice(data);
        }
        Self::wrap_frame(FRAME_TYPE_CONFIG, &payload)
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

        // STM32 expects float in big-endian format (matches read_float_be in protocol_handler.c)
        let temp_bytes = target_temp.to_be_bytes();
        payload.extend_from_slice(&temp_bytes);

        Self::wrap_frame(FRAME_TYPE_SET_TEMP, &payload)
    }

    /// 构建电机配置子帧 (CONFIG_SUBTYPE_MOTOR = 0x01)。
    /// payload: [0x01][sub_len][motor...]，每个 motor 固定 11 字节：
    ///   axis, step_port, step_pin, dir_port, dir_pin, dir_inverted,
    ///   en_port, en_pin, en_inverted, uart_port, uart_pin
    /// sub_len = 11 * motors.len()，与其它子帧的 [sub_type][sub_len][data] 布局保持一致，
    /// 使下位机可以统一走通用 TLV 解析循环。
    fn build_motor_frame(motors: &[&MotorParams]) -> Vec<u8> {
        const MOTOR_ENTRY_LEN: usize = 11;

        let mut payload = vec![CONFIG_SUBTYPE_MOTOR];
        payload.push((motors.len() * MOTOR_ENTRY_LEN) as u8);

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
            payload.push(uart.map(|p| p.port).unwrap_or(0xFF));
            payload.push(uart.map(|p| p.pin).unwrap_or(0xFF));
        }

        Self::wrap_frame(FRAME_TYPE_CONFIG, &payload)
    }

    fn build_limit_switch_frame(limit: &LimitSwitchParams, motors: &[MotorParams]) -> Vec<u8> {
        let mut payload = vec![0x03];  // CONFIG_SUB_LIMIT

        // Helper: find motor params by axis name
        let motor = |axis: &str| -> Option<&MotorParams> {
            motors.iter().find(|m| m.axis.eq_ignore_ascii_case(axis))
        };

        let xy_spmm = motor("X").map_or(80u32, |m| m.steps_per_mm);
        let z_spmm = motor("Z").map_or(400u32, |m| m.steps_per_mm);

        // Axis pin configs (8 bytes each, 28 bytes total: X/Y/Z have position_endstop, E = 4)
        payload.extend_from_slice(&Self::limit_axis_to_bytes(&limit.x));
        payload.extend_from_slice(&Self::limit_axis_to_bytes(&limit.y));
        payload.extend_from_slice(&Self::limit_axis_to_bytes(&limit.z));

        // E axis (4 bytes, no position_endstop in C struct)
        payload.extend_from_slice(&[0xFF, 0xFF, 0x00, 0x00]);

        // Homing direction (3 bytes) + reserved padding (1 byte)
        payload.push(limit.x.homing_dir);
        payload.push(limit.y.homing_dir);
        payload.push(limit.z.homing_dir);
        payload.push(0x00);  // _reserved0 padding

        // X axis homing params (16 bytes): speed/fine/retract/max_travel
        let x_max_travel = motor("X").map_or(u32::MAX, |m| {
            let v = (m.position_max - m.position_min).max(1) as f32 * 1.5 * m.steps_per_mm as f32;
            v as u32
        });
        payload.extend_from_slice(&((limit.x.homing_speed_mm_per_s as u32 * xy_spmm).to_be_bytes()));
        payload.extend_from_slice(&((limit.x.homing_fine_speed_mm_per_s as u32 * xy_spmm).to_be_bytes()));
        payload.extend_from_slice(&((limit.x.homing_retract_mm * xy_spmm as f32) as u32).to_be_bytes());
        payload.extend_from_slice(&x_max_travel.to_be_bytes());

        // Y axis homing params (16 bytes)
        let y_max_travel = motor("Y").map_or(u32::MAX, |m| {
            let v = (m.position_max - m.position_min).max(1) as f32 * 1.5 * m.steps_per_mm as f32;
            v as u32
        });
        payload.extend_from_slice(&((limit.y.homing_speed_mm_per_s as u32 * xy_spmm).to_be_bytes()));
        payload.extend_from_slice(&((limit.y.homing_fine_speed_mm_per_s as u32 * xy_spmm).to_be_bytes()));
        payload.extend_from_slice(&((limit.y.homing_retract_mm * xy_spmm as f32) as u32).to_be_bytes());
        payload.extend_from_slice(&y_max_travel.to_be_bytes());

        // Z axis homing params (16 bytes)
        let z_max_travel = motor("Z").map_or(u32::MAX, |m| {
            let v = (m.position_max - m.position_min).max(1) as f32 * 1.5 * m.steps_per_mm as f32;
            v as u32
        });
        payload.extend_from_slice(&((limit.z.homing_speed_mm_per_s as u32 * z_spmm).to_be_bytes()));
        payload.extend_from_slice(&((limit.z.homing_fine_speed_mm_per_s as u32 * z_spmm).to_be_bytes()));
        payload.extend_from_slice(&((limit.z.homing_retract_mm * z_spmm as f32) as u32).to_be_bytes());
        payload.extend_from_slice(&z_max_travel.to_be_bytes());

        // Global homing params: z_lift (4 bytes)
        let z_lift_steps = (limit.homing.z_lift_mm * z_spmm as f32) as u32;
        payload.extend_from_slice(&z_lift_steps.to_be_bytes());

        Self::wrap_frame(FRAME_TYPE_CONFIG, &payload)
    }

    fn limit_axis_to_bytes(axis: &LimitSwitchAxis) -> [u8; 8] {
        let pin = parse_pin(&axis.pin);
        let port = pin.map(|p| p.port).unwrap_or(0xFF);
        let pin_num = pin.map(|p| p.pin).unwrap_or(0xFF);
        let pull = match axis.pull.as_str() {
            "up" => 0x01,
            "down" => 0x02,
            _ => 0x00,
        };
        let active_high = if axis.active_high { 1 } else { 0 };
        let pos = axis.position_endstop.unwrap_or(0.0) as i32;

        [
            port,
            pin_num,
            pull,
            active_high,
            (pos >> 24) as u8,
            (pos >> 16) as u8,
            (pos >> 8) as u8,
            pos as u8,
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

        // Infer ntc_resistance_25c from sensor_type
        let ntc_resistance_25c = match temp.sensor_type.as_str() {
            "NTC100K" => 100000u32,
            "NTC50K" => 50000u32,
            "NTC10K" => 10000u32,
            _ => 100000u32, // Default to 100K
        };
        let r25_bytes = ntc_resistance_25c.to_be_bytes();
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

        // STM32 expects 1 byte for sensor_fault_threshold
        payload.push(heater.safety.sensor_fault_threshold as u8);

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
        // Max 15 chars to leave room for null terminator (char name[16] in firmware)
        let copy_len = name_bytes.len().min(15);
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
        // Max 15 chars to leave room for null terminator (char name[16] in firmware)
        let copy_len = name_bytes.len().min(15);
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

    /// 构建系统配置帧（CONFIG_SUBTYPE_SYSTEM = 0x05）
    /// status_report_interval_ms: 状态上报间隔（毫秒）
    fn build_system_frame(status_report_interval_ms: u32) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.push(CONFIG_SUBTYPE_SYSTEM);
        
        // 状态上报间隔（大端字节序）
        buf.extend_from_slice(&status_report_interval_ms.to_be_bytes());

        Self::wrap_frame(FRAME_TYPE_CONFIG, &buf)
    }

    /// 构建 PID 整定配置帧（热端）
    /// heater_id: 1 = 热端
    pub fn build_pid_tune_hotend_frame(config: &PidTuneHeaterConfig) -> Vec<u8> {
        Self::build_pid_tune_frame_impl(1, config)  // heater_id = 1
    }

    /// 构建 PID 整定配置帧（热床）
    /// heater_id: 0 = 热床
    pub fn build_pid_tune_hotbed_frame(config: &PidTuneHeaterConfig) -> Vec<u8> {
        Self::build_pid_tune_frame_impl(0, config)  // heater_id = 0
    }

    /// 内部实现：构建 PID 整定配置帧
    /// 帧格式:
    /// [SUB_TYPE:1][HEATER_ID:1][MAX_OVERTEMP:2][TIMEOUT:4]
    /// [POWER_DIV:4][SWITCH_DELAY:4][INIT_BIAS:4][INIT_D:4] = 24字节payload
    fn build_pid_tune_frame_impl(heater_id: u8, config: &PidTuneHeaterConfig) -> Vec<u8> {
        let mut payload = Vec::with_capacity(24);
        
        // 子帧类型
        payload.push(CONFIG_SUBTYPE_PID_TUNE);
        
        // 加热器ID
        payload.push(heater_id);
        
        // max_overtemp (f32 -> u16，大端序，放大10倍)
        let overtemp_raw = (config.max_overtemp * 10.0) as u16;
        payload.extend_from_slice(&overtemp_raw.to_be_bytes());
        
        // timeout_ms (u32, 大端序)
        payload.extend_from_slice(&config.timeout_ms.to_be_bytes());
        
        // power_divisor (u32, 大端序)
        payload.extend_from_slice(&config.power_divisor.to_be_bytes());
        
        // switch_delay_ms (u32, 大端序)
        payload.extend_from_slice(&config.switch_delay_ms.to_be_bytes());
        
        // initial_bias (u32, 大端序)
        payload.extend_from_slice(&config.initial_bias.to_be_bytes());
        
        // initial_d (u32, 大端序)
        payload.extend_from_slice(&config.initial_d.to_be_bytes());

        Self::wrap_frame(FRAME_TYPE_CONFIG, &payload)
    }

    // ============ TMC2209 独立配置帧 ============

    /// 为所有归属于 MCU1 的轴构建 TMC2209 静态配置帧 (0x2A)。
    /// payload 布局对应固件 TmcConfigPayload (emb_protocol.h):
    ///   [0] axis  [1] uart_addr  [2] irun  [3] ihold  [4] iholddelay
    ///   [5] tpowerdown  [6] mres  [7] intpol  [8] en_spreadcycle  [9] vsense
    ///   [10] toff  [11] hstrt  [12] hend  [13] tbl  [14..17] tpwmthrs(LE)
    ///   [18] pwm_auto_scale  [19] pwm_auto_grad
    fn build_tmc_config_frames(motors: &[MotorParams]) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for motor in motors {
            // 仅下发 MCU1 上的轴（MCU2 上的 E 轴由 MCU2 侧管理）
            if !motor.mcu.eq_ignore_ascii_case("MCU1") {
                continue;
            }
            // 未配置 uart_pin 的轴不需要下发 TMC 配置（无 UART 通信能力，
            // 下发后下位机也无法 ACK，会触发 ACK 超时重传）。
            if motor.driver.uart_pin.trim().is_empty() {
                tracing::debug!(
                    "跳过轴 {} 的 TMC 配置下发：未配置 uart_pin",
                    motor.axis
                );
                continue;
            }
            let axis = match motor.axis.as_bytes().first().copied().unwrap_or(0) {
                b'X' => Some(TMC_AXIS_X),
                b'Y' => Some(TMC_AXIS_Y),
                b'Z' => Some(TMC_AXIS_Z),
                _ => None,
            };
            let axis = match axis {
                Some(a) => a,
                None => continue,
            };
            let d = &motor.driver;
            frames.push(Self::build_tmc_config_frame(axis, d));
        }
        frames
    }

    fn build_tmc_config_frame(axis: u8, d: &DriverParams) -> Vec<u8> {
        let irun = Self::calc_irun(d);
        let ihold = Self::calc_ihold(d);
        let mres = Self::microsteps_to_mres(d.microsteps);

        let mut payload = Vec::with_capacity(23);
        payload.push(axis);                    // [0] axis
        payload.push(d.uart_addr.min(3));      // [1] uart_addr
        payload.push(irun);                    // [2] irun
        payload.push(ihold);                   // [3] ihold
        payload.push(d.iholddelay.min(15));    // [4] iholddelay
        payload.push(d.tpowerdown);            // [5] tpowerdown
        payload.push(mres);                    // [6] mres
        payload.push(if d.intpol != 0 { 1 } else { 0 });  // [7] intpol
        // [8] en_spreadcycle: 由 stealthchop_threshold 决定。若为 0 表示始终 StealthChop(0)，否则 SpreadCycle(1)
        payload.push(if d.stealthchop_threshold == 0 { 0 } else { 1 });
        payload.push(if d.vsense != 0 { 1 } else { 0 });  // [9] vsense
        payload.push(d.toff.min(15));          // [10] toff
        payload.push(d.hstrt.min(7));          // [11] hstrt
        payload.push(d.hend.min(15));          // [12] hend
        payload.push(d.tbl.min(3));            // [13] tbl
        // [14..17] tpwmthrs (LE)
        payload.extend_from_slice(&d.tpwmthrs.to_le_bytes());
        payload.push(d.pwm_auto_scale.min(15));  // [18]
        payload.push(d.pwm_auto_grad.min(15));   // [19]
        // [20] homing_mode: 0=limit 机械, 1=sensorless diag
        payload.push(Self::homing_mode_byte(d));
        // [21] diag_port: 0=无, 1=A,2=B,3=C,4=D
        payload.push(Self::diag_port_byte(&d.diag_pin));
        // [22] diag_pin: 0~15, 0xFF=未配置
        payload.push(Self::diag_pin_byte(&d.diag_pin));

        Self::wrap_frame(FRAME_TYPE_TMC_CONFIG, &payload)
    }

    /// homing_mode 字符串 → 协议字节 (0=limit, 1=sensorless)
    fn homing_mode_byte(d: &DriverParams) -> u8 {
        if d.homing_mode.eq_ignore_ascii_case("sensorless") { 1 } else { 0 }
    }

    /// "PA8"/"PB3" 等引脚串 → 端口编码 (0=无, 1=A,2=B,3=C,4=D)
    fn diag_port_byte(pin: &str) -> u8 {
        let s = pin.trim();
        if s.is_empty() { return 0; }
        match s.as_bytes().get(0).copied().unwrap_or(0).to_ascii_uppercase() {
            b'A' => 1, b'B' => 2, b'C' => 3, b'D' => 4, _ => 0,
        }
    }

    /// "PA8" 等引脚串 → 引脚号 (0~15)，未配置/非法返回 0xFF
    fn diag_pin_byte(pin: &str) -> u8 {
        let s = pin.trim();
        if s.len() < 2 || s.len() > 3 { return 0xFF; }
        let num = &s[1..];
        match num.parse::<u8>() {
            Ok(v) if v <= 15 => v,
            _ => 0xFF,
        }
    }

    /// 构建 StallGuard 配置帧 (0x28) 用于传感器无源归位。
    /// 由归位流程在归位前 (enable=1) 与归位后 (enable=0) 调用。
    /// 返回对应 MCU1 上 X/Y 轴的帧列表。
    /// payload 布局对应固件 TmcStallPayload (emb_protocol.h):
    ///   [0] axis  [1] enable  [2] sgthrs  [3..5] tcoolthrs(LE24)
    pub fn build_tmc_stall_cfg_frames(motors: &[MotorParams], enable: u8) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        for motor in motors {
            if !motor.mcu.eq_ignore_ascii_case("MCU1") {
                continue;
            }
            // 仅 X/Y 支持传感器无源归位 (DIAG)，且需配置为 sensorless 归位方式
            let d = &motor.driver;
            if !d.homing_mode.eq_ignore_ascii_case("sensorless") {
                continue;
            }
            let axis = match motor.axis.as_bytes().first().copied().unwrap_or(0) {
                b'X' => Some(TMC_AXIS_X),
                b'Y' => Some(TMC_AXIS_Y),
                _ => None,
            };
            let axis = match axis {
                Some(a) => a,
                None => continue,
            };
            let mut payload = Vec::with_capacity(7);
            payload.push(axis);                     // [0] axis
            payload.push(enable);                   // [1] enable
            // 归位前 enable=1 需提供 sgthrs/tcoolthrs；归位后 enable=0 可省略
            payload.push(d.sgthrs);                 // [2] sgthrs
            // [3..6] tcoolthrs (LE24, 固件 TmcStallPayload.tcoolthrs 为 uint32, sizeof=7)
            payload.extend_from_slice(&d.tcoolthrs.to_le_bytes()[..3]);
            payload.push(0x00);                     // [6] tcoolthrs 高字节补零
            frames.push(Self::wrap_frame(FRAME_TYPE_TMC_STALL_CFG, &payload));
        }
        frames
    }

    /// 由 RMS 电流 (mA) + 采样电阻换算 IRUN (0~31)。
    /// TMC2209 数据手册: I_RMS = (IRUN+1)/32 * V_FS / (1.414 * (R_sense+0.02))
    /// 其中 V_FS = 0.32 (vsense=0) 或 0.18 (vsense=1)
    fn calc_irun(d: &DriverParams) -> u8 {
        let vfs = if d.vsense != 0 { 0.18 } else { 0.32 };
        let irun = (d.current_ma as f32 / 1000.0) * 32.0 * 1.414 * (d.sense_resistor + 0.02) / vfs - 1.0;
        irun.round().clamp(0.0, 31.0) as u8
    }

    /// 由保持电流换算 IHOLD (0~31)
    fn calc_ihold(d: &DriverParams) -> u8 {
        let vfs = if d.vsense != 0 { 0.18 } else { 0.32 };
        let ihold = (d.hold_current_ma as f32 / 1000.0) * 32.0 * 1.414 * (d.sense_resistor + 0.02) / vfs - 1.0;
        ihold.round().clamp(0.0, 31.0) as u8
    }

    /// microsteps -> MRES (0=256μstep ... 8=full-step)
    fn microsteps_to_mres(microsteps: u8) -> u8 {
        let ms = (microsteps as u32).clamp(1, 256);
        // MRES = 8 - log2(microsteps): 256→0, 128→1, 64→2, ..., 1→8
        (8 - ms.ilog2() as u8).clamp(0, 8)
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

    // Validate GPIO name length (max 15 chars to fit char name[16] with null terminator)
    for (i, pin) in config.gpio.output.iter().enumerate() {
        if pin.name.len() > 15 {
            return Err(format!(
                "GPIO output pin {} name '{}' is too long (max 15 chars, got {})",
                i, pin.name, pin.name.len()
            ));
        }
    }

    for (i, pin) in config.gpio.input.iter().enumerate() {
        if pin.name.len() > 15 {
            return Err(format!(
                "GPIO input pin {} name '{}' is too long (max 15 chars, got {})",
                i, pin.name, pin.name.len()
            ));
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

