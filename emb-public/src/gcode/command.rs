use crate::gcode::parser::{GCodeCommand, GCodeCategory};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GCodeCommandType {
    G0,
    G1,
    G28,
    G29,
    M104,
    M109,
    M140,
    M190,
    M106,
    M107,
    M84,
    Unknown,
}

impl GCodeCommand {
    pub fn command_type(&self) -> GCodeCommandType {
        match (self.letter, self.number) {
            ('G', 0) => GCodeCommandType::G0,
            ('G', 1) => GCodeCommandType::G1,
            ('G', 28) => GCodeCommandType::G28,
            ('G', 29) => GCodeCommandType::G29,
            ('M', 104) => GCodeCommandType::M104,
            ('M', 109) => GCodeCommandType::M109,
            ('M', 140) => GCodeCommandType::M140,
            ('M', 190) => GCodeCommandType::M190,
            ('M', 106) => GCodeCommandType::M106,
            ('M', 107) => GCodeCommandType::M107,
            ('M', 84) => GCodeCommandType::M84,
            _ => GCodeCommandType::Unknown,
        }
    }
    
    pub fn category_from_type(&self) -> GCodeCategory {
        match self.command_type() {
            GCodeCommandType::G0 => GCodeCategory::RapidPositioning,
            GCodeCommandType::G1 => GCodeCategory::LinearMove,
            GCodeCommandType::G28 => GCodeCategory::HomeAxes,
            GCodeCommandType::G29 => GCodeCategory::AutoBedLeveling,
            GCodeCommandType::M104 | GCodeCommandType::M109 | GCodeCommandType::M140 | GCodeCommandType::M190 => {
                GCodeCategory::TemperatureControl
            }
            GCodeCommandType::M106 | GCodeCommandType::M107 => GCodeCategory::FanControl,
            GCodeCommandType::M84 => GCodeCategory::MotorDisable,
            _ => GCodeCategory::Unknown,
        }
    }
}