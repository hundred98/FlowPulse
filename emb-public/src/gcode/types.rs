//! G-code data types
//!
//! This module defines the data structures for G-code parsing.

use serde::{Deserialize, Serialize};

/// 解析后的命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedCommand {
    /// 行号
    pub line_number: u32,
    /// 原始字符串
    pub raw: String,
    /// 命令类型
    pub kind: CommandKind,
}

/// 命令类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandKind {
    /// G指令 - 运动相关
    Motion(MotionCommand),
    /// M指令 - 机器控制
    Machine(emb_api::MCommand),
    /// 注释或空行
    Empty,
    /// 不支持的命令
    Unsupported { raw: String },
}

/// 运动命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MotionCommand {
    /// 线性移动 (G0/G1)
    LinearMove {
        x: Option<f32>,
        y: Option<f32>,
        z: Option<f32>,
        e: Option<f32>,
        f: Option<f32>,
        is_rapid: bool,
    },
    /// 圆弧移动 (G2/G3)
    ArcMove {
        x: Option<f32>,
        y: Option<f32>,
        z: Option<f32>,
        e: Option<f32>,
        f: Option<f32>,
        i: f32,
        j: f32,
        is_cw: bool,
    },
    /// 回零 (G28)
    Home {
        x: bool,
        y: bool,
        z: bool,
    },
    /// 设置位置 (G92)
    SetPosition {
        x: Option<f32>,
        y: Option<f32>,
        z: Option<f32>,
        e: Option<f32>,
    },
    /// 绝对定位 (G90)
    AbsolutePositioning,
    /// 相对定位 (G91)
    RelativePositioning,
    /// 英寸单位 (G20)
    Inches,
    /// 毫米单位 (G21)
    Millimeters,
}

/// 运动参数（服务端维护）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionParams {
    /// 最大速度 (mm/s) - M203设置
    pub max_velocity: [f32; 4], // [X, Y, Z, E]
    /// 加速度 (mm/s²) - M201设置
    pub acceleration: [f32; 4], // [X, Y, Z, E]
    /// 加速度参数 - M204设置
    pub accel_params: AccelParams,
    /// 每毫米步数 - M92设置
    pub steps_per_mm: [f32; 4], // [X, Y, Z, E]
}

impl Default for MotionParams {
    fn default() -> Self {
        Self {
            max_velocity: [300.0, 300.0, 10.0, 60.0], // 默认最大速度
            acceleration: [3000.0, 3000.0, 100.0, 5000.0], // 默认加速度
            accel_params: AccelParams::default(),
            steps_per_mm: [80.0, 80.0, 400.0, 100.0], // 默认步数
        }
    }
}

/// 加速度参数 - M204
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccelParams {
    /// 移动加速度 (mm/s²)
    pub travel: f32,
    /// 打印加速度 (mm/s²)
    pub print: f32,
    /// 回抽加速度 (mm/s²)
    pub retract: f32,
}

impl Default for AccelParams {
    fn default() -> Self {
        Self {
            travel: 3000.0,
            print: 1500.0,
            retract: 3000.0,
        }
    }
}

impl MotionParams {
    /// 创建新的运动参数
    pub fn new() -> Self {
        Self::default()
    }

    /// 应用M201加速度设置
    pub fn apply_acceleration(&mut self, x: Option<f32>, y: Option<f32>, z: Option<f32>, e: Option<f32>) {
        if let Some(v) = x { self.acceleration[0] = v; }
        if let Some(v) = y { self.acceleration[1] = v; }
        if let Some(v) = z { self.acceleration[2] = v; }
        if let Some(v) = e { self.acceleration[3] = v; }
    }

    /// 应用M203最大速度设置
    pub fn apply_max_velocity(&mut self, x: Option<f32>, y: Option<f32>, z: Option<f32>, e: Option<f32>) {
        if let Some(v) = x { self.max_velocity[0] = v; }
        if let Some(v) = y { self.max_velocity[1] = v; }
        if let Some(v) = z { self.max_velocity[2] = v; }
        if let Some(v) = e { self.max_velocity[3] = v; }
    }

    /// 应用M204加速度参数
    pub fn apply_accel_params(&mut self, travel: Option<f32>, print: Option<f32>, retract: Option<f32>) {
        if let Some(v) = travel { self.accel_params.travel = v; }
        if let Some(v) = print { self.accel_params.print = v; }
        if let Some(v) = retract { self.accel_params.retract = v; }
    }

    /// 应用M92步数设置
    pub fn apply_steps_per_mm(&mut self, x: Option<f32>, y: Option<f32>, z: Option<f32>, e: Option<f32>) {
        if let Some(v) = x { self.steps_per_mm[0] = v; }
        if let Some(v) = y { self.steps_per_mm[1] = v; }
        if let Some(v) = z { self.steps_per_mm[2] = v; }
        if let Some(v) = e { self.steps_per_mm[3] = v; }
    }

    /// 获取受限的速度（取 min(feed_rate, max_velocity)）
    pub fn get_limited_speed(&self, axis: usize, feed_rate: f32) -> f32 {
        if axis < 4 {
            feed_rate.min(self.max_velocity[axis])
        } else {
            feed_rate
        }
    }
}
