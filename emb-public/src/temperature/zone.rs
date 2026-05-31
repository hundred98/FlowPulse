//! Heater zone management - synchronous version

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TemperatureState {
    Idle,
    Heating,
    Stable,
    Cooling,
    AlarmOverTemp,
    AlarmSensorError,
    AlarmTimeout,
}

impl TemperatureState {
    pub fn is_alarm(&self) -> bool {
        matches!(self,
            TemperatureState::AlarmOverTemp |
            TemperatureState::AlarmSensorError |
            TemperatureState::AlarmTimeout
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(self, TemperatureState::Heating | TemperatureState::Stable)
    }

    pub fn description(&self) -> &'static str {
        match self {
            TemperatureState::Idle => "空闲",
            TemperatureState::Heating => "加热中",
            TemperatureState::Stable => "温度稳定",
            TemperatureState::Cooling => "降温中",
            TemperatureState::AlarmOverTemp => "超温告警",
            TemperatureState::AlarmSensorError => "传感器故障",
            TemperatureState::AlarmTimeout => "通信超时",
        }
    }
}

impl Default for TemperatureState {
    fn default() -> Self {
        TemperatureState::Idle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PidParams {
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
}

impl Default for PidParams {
    fn default() -> Self {
        Self {
            kp: 22.2,
            ki: 1.08,
            kd: 114.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaterConfig {
    pub name: String,
    pub max_temp: f32,
    pub pid: PidParams,
    pub sample_period_ms: u16,
}

impl HeaterConfig {
    pub fn hotend_default() -> Self {
        Self {
            name: "hotend".to_string(),
            max_temp: 300.0,
            pid: PidParams::default(),
            sample_period_ms: 100,
        }
    }

    pub fn bed_default() -> Self {
        Self {
            name: "bed".to_string(),
            max_temp: 120.0,
            pid: PidParams {
                kp: 10.0,
                ki: 0.5,
                kd: 50.0,
            },
            sample_period_ms: 500,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TemperatureStatus {
    pub current: f32,
    pub target: f32,
    pub power: f32,
    pub state: TemperatureState,
    pub last_update: Instant,
}

impl TemperatureStatus {
    pub fn new(current: f32, target: f32, power: f32) -> Self {
        let state = Self::calculate_state(current, target, power);
        Self {
            current,
            target,
            power,
            state,
            last_update: Instant::now(),
        }
    }

    fn calculate_state(current: f32, target: f32, _power: f32) -> TemperatureState {
        if target <= 0.0 {
            if current < 40.0 {
                TemperatureState::Idle
            } else {
                TemperatureState::Cooling
            }
        } else if (current - target).abs() <= 2.0 {
            TemperatureState::Stable
        } else if current < target {
            TemperatureState::Heating
        } else {
            TemperatureState::Cooling
        }
    }

    pub fn is_stable(&self, tolerance: f32) -> bool {
        (self.current - self.target).abs() <= tolerance
    }

    pub fn is_safe(&self, max_temp: f32) -> bool {
        self.current <= max_temp
    }

    pub fn deviation(&self) -> f32 {
        self.current - self.target
    }

    pub fn is_stale(&self, timeout: Duration) -> bool {
        self.last_update.elapsed() > timeout
    }
}

impl Default for TemperatureStatus {
    fn default() -> Self {
        Self {
            current: 20.0,
            target: 0.0,
            power: 0.0,
            state: TemperatureState::Idle,
            last_update: Instant::now(),
        }
    }
}

#[derive(Debug)]
pub struct HeaterZone {
    pub config: HeaterConfig,
    pub status: TemperatureStatus,
    target_temp: f32,
    enabled: bool,
    alarm_triggered: bool,
}

impl HeaterZone {
    pub fn new(config: HeaterConfig) -> Self {
        Self {
            status: TemperatureStatus::default(),
            target_temp: 0.0,
            enabled: false,
            alarm_triggered: false,
            config,
        }
    }

    pub fn set_target(&mut self, temp: f32) {
        self.target_temp = temp.clamp(0.0, self.config.max_temp);
    }

    pub fn target(&self) -> f32 {
        self.target_temp
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.target_temp = 0.0;
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn update_status(&mut self, current: f32, power: f32) {
        self.status = TemperatureStatus::new(current, self.target_temp, power);
        if !self.status.is_safe(self.config.max_temp) && self.enabled {
            self.alarm_triggered = true;
        }
    }

    pub fn is_alarm(&self) -> bool {
        self.alarm_triggered
    }

    pub fn clear_alarm(&mut self) {
        self.alarm_triggered = false;
        self.set_enabled(false);
    }

    pub fn wait_for_temp(&self, tolerance: f32, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.status.is_stable(tolerance) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }
}