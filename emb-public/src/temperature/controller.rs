//! Temperature Controller - manages all heater zones (sync version)

use crate::common::{EmbError, EmbResult};
use crate::gcode::{GCodeCommand, GCodeCategory};
use crate::temperature::zone::{HeaterZone, HeaterConfig, TemperatureStatus, PidParams};
use std::collections::HashMap;
use std::time::Duration;

pub struct TemperatureController {
    heaters: HashMap<String, HeaterZone>,
    default_hotend: String,
    default_bed: String,
    status_timeout: Duration,
}

impl TemperatureController {
    pub fn new() -> Self {
        let mut heaters = HashMap::new();

        let hotend = HeaterZone::new(HeaterConfig::hotend_default());
        heaters.insert("hotend".to_string(), hotend);

        let bed = HeaterZone::new(HeaterConfig::bed_default());
        heaters.insert("bed".to_string(), bed);

        Self {
            heaters,
            default_hotend: "hotend".to_string(),
            default_bed: "bed".to_string(),
            status_timeout: Duration::from_secs(5),
        }
    }

    pub fn add_heater(&mut self, name: &str, config: HeaterConfig) {
        let heater = HeaterZone::new(config);
        self.heaters.insert(name.to_string(), heater);
    }

    pub fn heater(&self, name: &str) -> Option<&HeaterZone> {
        self.heaters.get(name)
    }

    pub fn heater_mut(&mut self, name: &str) -> Option<&mut HeaterZone> {
        self.heaters.get_mut(name)
    }

    pub fn hotend(&self) -> Option<&HeaterZone> {
        self.heaters.get(&self.default_hotend)
    }

    pub fn hotend_mut(&mut self) -> Option<&mut HeaterZone> {
        self.heaters.get_mut(&self.default_hotend)
    }

    pub fn bed(&self) -> Option<&HeaterZone> {
        self.heaters.get(&self.default_bed)
    }

    pub fn bed_mut(&mut self) -> Option<&mut HeaterZone> {
        self.heaters.get_mut(&self.default_bed)
    }

    pub fn set_hotend_temp(&mut self, temp: f32, index: Option<u8>) -> EmbResult<()> {
        let name = if let Some(idx) = index {
            format!("hotend{}", idx)
        } else {
            self.default_hotend.clone()
        };

        if let Some(heater) = self.heaters.get_mut(&name) {
            heater.set_target(temp);
            heater.set_enabled(temp > 0.0);
            Ok(())
        } else {
            Err(EmbError::Configuration(format!("Heater '{}' not found", name)))
        }
    }

    pub fn set_bed_temp(&mut self, temp: f32) -> EmbResult<()> {
        if let Some(heater) = self.bed_mut() {
            heater.set_target(temp);
            heater.set_enabled(temp > 0.0);
            Ok(())
        } else {
            Err(EmbError::Configuration("Bed heater not found".to_string()))
        }
    }

    pub fn wait_for_hotend(&self, tolerance: f32, timeout: Duration, index: Option<u8>) -> EmbResult<bool> {
        let name = if let Some(idx) = index {
            format!("hotend{}", idx)
        } else {
            self.default_hotend.clone()
        };

        if let Some(heater) = self.heaters.get(&name) {
            Ok(heater.wait_for_temp(tolerance, timeout))
        } else {
            Err(EmbError::Configuration(format!("Heater '{}' not found", name)))
        }
    }

    pub fn wait_for_bed(&self, tolerance: f32, timeout: Duration) -> EmbResult<bool> {
        if let Some(heater) = self.bed() {
            Ok(heater.wait_for_temp(tolerance, timeout))
        } else {
            Err(EmbError::Configuration("Bed heater not found".to_string()))
        }
    }

    pub fn process_gcode(&mut self, cmd: &GCodeCommand) -> EmbResult<()> {
        let category = GCodeCategory::from_command(cmd.letter, cmd.number);
        if category != GCodeCategory::TemperatureControl {
            return Ok(());
        }

        match cmd.letter {
            'M' => match cmd.number {
                104 => {
                    let temp = cmd.s().unwrap_or(0.0);
                    let index = cmd.t().map(|t| t as u8);
                    self.set_hotend_temp(temp, index)?;
                }
                109 => {
                    let temp = cmd.s().unwrap_or(0.0);
                    let index = cmd.t().map(|t| t as u8);
                    self.set_hotend_temp(temp, index)?;
                    self.wait_for_hotend(2.0, Duration::from_secs(600), index)?;
                }
                140 => {
                    let temp = cmd.s().unwrap_or(0.0);
                    self.set_bed_temp(temp)?;
                }
                190 => {
                    let temp = cmd.s().unwrap_or(0.0);
                    self.set_bed_temp(temp)?;
                    self.wait_for_bed(2.0, Duration::from_secs(600))?;
                }
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }

    pub fn update_temperature(&mut self, name: &str, current: f32, power: f32) -> EmbResult<()> {
        if let Some(heater) = self.heaters.get_mut(name) {
            heater.update_status(current, power);
            if heater.is_alarm() {
                return Err(EmbError::Configuration(
                    format!("Over-temperature alarm on '{}'", name)
                ));
            }
            Ok(())
        } else {
            Err(EmbError::Configuration(format!("Heater '{}' not found", name)))
        }
    }

    pub fn all_temperatures(&self) -> Vec<(&str, &TemperatureStatus)> {
        self.heaters
            .iter()
            .map(|(name, heater)| (name.as_str(), &heater.status))
            .collect()
    }

    pub fn has_stale_status(&self) -> Vec<String> {
        self.heaters
            .iter()
            .filter(|(_, heater)| heater.status.is_stale(self.status_timeout))
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub fn all_at_temp(&self, tolerance: f32) -> bool {
        self.heaters
            .values()
            .filter(|h| h.is_enabled())
            .all(|h| h.status.is_stable(tolerance))
    }

    pub fn disable_all(&mut self) {
        for heater in self.heaters.values_mut() {
            heater.set_enabled(false);
        }
    }

    pub fn get_pid(&self, name: &str) -> Option<PidParams> {
        self.heater(name).map(|h| h.config.pid)
    }

    pub fn set_pid(&mut self, name: &str, pid: PidParams) -> EmbResult<()> {
        if let Some(heater) = self.heaters.get_mut(name) {
            heater.config.pid = pid;
            Ok(())
        } else {
            Err(EmbError::Configuration(format!("Heater '{}' not found", name)))
        }
    }

    pub fn check_alarms(&self) -> Vec<String> {
        self.heaters
            .iter()
            .filter(|(_, h)| h.is_alarm())
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub fn clear_all_alarms(&mut self) {
        for heater in self.heaters.values_mut() {
            if heater.is_alarm() {
                heater.clear_alarm();
            }
        }
    }
}

impl Default for TemperatureController {
    fn default() -> Self {
        Self::new()
    }
}