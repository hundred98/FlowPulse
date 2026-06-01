//! G-code Parser
//!
//! Provides G-code parsing capabilities for 3D printers.

use crate::common::{EmbError, EmbResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GCodeCommand {
    pub letter: char,
    pub number: u16,
    pub params: HashMap<char, f32>,
    pub raw_line: String,
    pub line_number: usize,
    pub checksum: Option<u8>,
}

impl GCodeCommand {
    pub fn new(letter: char, number: u16) -> Self {
        Self {
            letter,
            number,
            params: HashMap::new(),
            raw_line: String::new(),
            line_number: 0,
            checksum: None,
        }
    }

    pub fn with_param(mut self, key: char, value: f32) -> Self {
        self.params.insert(key, value);
        self
    }

    pub fn get_param(&self, key: char) -> Option<f32> {
        self.params.get(&key).copied()
    }

    pub fn x(&self) -> Option<f32> { self.get_param('X') }
    pub fn y(&self) -> Option<f32> { self.get_param('Y') }
    pub fn z(&self) -> Option<f32> { self.get_param('Z') }
    pub fn e(&self) -> Option<f32> { self.get_param('E') }
    pub fn f(&self) -> Option<f32> { self.get_param('F') }
    pub fn s(&self) -> Option<f32> { self.get_param('S') }
    pub fn t(&self) -> Option<f32> { self.get_param('T') }
    pub fn i(&self) -> Option<f32> { self.get_param('I') }
    pub fn j(&self) -> Option<f32> { self.get_param('J') }
    pub fn r(&self) -> Option<f32> { self.get_param('R') }
    pub fn p(&self) -> Option<f32> { self.get_param('P') }

    pub fn is_movement(&self) -> bool {
        self.letter == 'G' && (self.number == 0 || self.number == 1)
    }

    pub fn is_rapid_move(&self) -> bool {
        self.letter == 'G' && self.number == 0
    }

    pub fn is_linear_move(&self) -> bool {
        self.letter == 'G' && self.number == 1
    }

    pub fn is_arc_cw(&self) -> bool {
        self.letter == 'G' && self.number == 2
    }

    pub fn is_arc_ccw(&self) -> bool {
        self.letter == 'G' && self.number == 3
    }

    pub fn is_arc_move(&self) -> bool {
        self.is_arc_cw() || self.is_arc_ccw()
    }

    pub fn is_arc_ij_format(&self) -> bool {
        self.i().is_some() || self.j().is_some()
    }

    pub fn is_arc_r_format(&self) -> bool {
        self.r().is_some() && !self.is_arc_ij_format()
    }

    pub fn calculate_checksum(&self) -> u8 {
        let mut checksum: u8 = 0;
        for byte in self.raw_line.bytes() {
            checksum ^= byte;
        }
        checksum
    }

    pub fn verify_checksum(&self) -> bool {
        if let Some(expected) = self.checksum {
            self.calculate_checksum() == expected
        } else {
            true
        }
    }

    pub fn category(&self) -> GCodeCategory {
        match (self.letter, self.number) {
            ('G', 0) => GCodeCategory::RapidPositioning,
            ('G', 1) => GCodeCategory::LinearMove,
            ('G', 2) => GCodeCategory::ArcCW,
            ('G', 3) => GCodeCategory::ArcCCW,
            ('G', 4) => GCodeCategory::Dwell,
            ('G', 20) => GCodeCategory::SetInches,
            ('G', 21) => GCodeCategory::SetMillimeters,
            ('G', 90) => GCodeCategory::SetAbsolute,
            ('G', 91) => GCodeCategory::SetRelative,
            ('G', 28) => GCodeCategory::HomeAxes,
            ('G', 29) => GCodeCategory::AutoBedLeveling,
            ('G', 92) => GCodeCategory::SetPosition,
            ('M', 104) | ('M', 109) => GCodeCategory::TemperatureControl,
            ('M', 106) | ('M', 107) => GCodeCategory::FanControl,
            ('M', 140) | ('M', 190) => GCodeCategory::TemperatureControl,
            ('M', 83) | ('M', 82) => GCodeCategory::Extruder,
            ('M', 84) => GCodeCategory::MotorDisable,
            ('M', 900) => GCodeCategory::PressureAdvance,
            _ => GCodeCategory::Unknown,
        }
    }
}

impl std::fmt::Display for GCodeCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.letter, self.number)?;
        for (key, value) in &self.params {
            if value.fract() == 0.0 {
                write!(f, " {}{:.0}", key, value)?;
            } else {
                write!(f, " {}{:.3}", key, value)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GCodeCategory {
    RapidPositioning,
    LinearMove,
    ArcCW,
    ArcCCW,
    Dwell,
    SetInches,
    SetMillimeters,
    SetAbsolute,
    SetRelative,
    HomeAxes,
    AutoBedLeveling,
    SetPosition,
    TemperatureControl,
    FanControl,
    Extruder,
    BedControl,
    PressureAdvance,
    MotorDisable,
    Unknown,
}

impl GCodeCategory {
    pub fn from_command(letter: char, number: u16) -> Self {
        match (letter, number) {
            ('G', 0) => GCodeCategory::RapidPositioning,
            ('G', 1) => GCodeCategory::LinearMove,
            ('G', 2) => GCodeCategory::ArcCW,
            ('G', 3) => GCodeCategory::ArcCCW,
            ('G', 4) => GCodeCategory::Dwell,
            ('G', 20) => GCodeCategory::SetInches,
            ('G', 21) => GCodeCategory::SetMillimeters,
            ('G', 90) => GCodeCategory::SetAbsolute,
            ('G', 91) => GCodeCategory::SetRelative,
            ('G', 28) => GCodeCategory::HomeAxes,
            ('G', 29) => GCodeCategory::AutoBedLeveling,
            ('G', 92) => GCodeCategory::SetPosition,
            ('M', 104) | ('M', 109) => GCodeCategory::TemperatureControl,
            ('M', 106) | ('M', 107) => GCodeCategory::FanControl,
            ('M', 140) | ('M', 190) => GCodeCategory::TemperatureControl,
            ('M', 83) | ('M', 82) => GCodeCategory::Extruder,
            ('M', 900) => GCodeCategory::PressureAdvance,
            _ => GCodeCategory::Unknown,
        }
    }
}

pub struct GCodeParser {
    pub line_number: usize,
    pub strict_mode: bool,
    pub support_line_numbers: bool,
}

impl GCodeParser {
    pub fn new() -> Self {
        Self {
            line_number: 0,
            strict_mode: false,
            support_line_numbers: true,
        }
    }

    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    pub fn with_line_numbers(mut self, enabled: bool) -> Self {
        self.support_line_numbers = enabled;
        self
    }

    pub fn parse_line(&mut self, line: &str) -> EmbResult<Option<GCodeCommand>> {
        self.line_number += 1;

        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with(';') {
            return Ok(None);
        }

        let line_without_comment = if let Some(pos) = trimmed.find(';') {
            &trimmed[..pos]
        } else {
            trimmed
        };

        let mut content = line_without_comment.trim();

        if self.support_line_numbers && content.starts_with('N') {
            if let Some(space_pos) = content.find(' ') {
                let num_str = &content[1..space_pos];
                if num_str.parse::<usize>().is_ok() {
                    content = &content[space_pos..].trim();
                }
            }
        }

        let mut checksum: Option<u8> = None;
        if let Some(star_pos) = content.rfind('*') {
            let check_str = &content[star_pos + 1..];
            if let Ok(check) = u8::from_str_radix(check_str, 16) {
                checksum = Some(check);
                content = &content[..star_pos].trim();
            }
        }

        if content.is_empty() {
            return Ok(None);
        }

        let first_char = content.chars().next().unwrap();
        if !first_char.is_ascii_alphabetic() {
            return Err(EmbError::GCodeParse(format!(
                "Line {}: Expected command letter, got '{}'",
                self.line_number, first_char
            )));
        }

        let mut number_end = 1;
        while number_end < content.len() {
            let c = content.chars().nth(number_end).unwrap();
            if !c.is_ascii_digit() && c != '.' {
                break;
            }
            number_end += 1;
        }

        let letter = first_char.to_ascii_uppercase();
        let number_str = &content[1..number_end];
        let number = number_str.parse::<u16>().map_err(|_| {
            EmbError::GCodeParse(format!(
                "Line {}: Invalid command number '{}'",
                self.line_number, number_str
            ))
        })?;

        let mut params = HashMap::new();
        let param_content = &content[number_end..];

        let mut i = 0;
        while i < param_content.len() {
            while i < param_content.len() && param_content.chars().nth(i).unwrap().is_whitespace() {
                i += 1;
            }
            if i >= param_content.len() {
                break;
            }

            let param_char = param_content.chars().nth(i).unwrap();
            if !param_char.is_ascii_alphabetic() {
                i += 1;
                continue;
            }

            let value_start = i + 1;
            let mut value_end = value_start;

            while value_end < param_content.len() {
                let c = param_content.chars().nth(value_end).unwrap();
                if c.is_ascii_alphabetic() || c.is_whitespace() {
                    break;
                }
                value_end += 1;
            }

            if value_start < value_end {
                let value_str = &param_content[value_start..value_end];
                if let Ok(value) = value_str.parse::<f32>() {
                    params.insert(param_char.to_ascii_uppercase(), value);
                }
            }

            i = value_end;
        }

        let cmd = GCodeCommand {
            letter,
            number,
            params,
            raw_line: line_without_comment.trim().to_string(),
            line_number: self.line_number,
            checksum,
        };

        if self.strict_mode && checksum.is_some() && !cmd.verify_checksum() {
            return Err(EmbError::GCodeParse(format!(
                "Line {}: Checksum mismatch",
                self.line_number
            )));
        }

        Ok(Some(cmd))
    }

    pub fn parse_lines(&mut self, lines: &[&str]) -> EmbResult<Vec<GCodeCommand>> {
        let mut commands = Vec::new();

        for line in lines {
            if let Some(cmd) = self.parse_line(line)? {
                commands.push(cmd);
            }
        }

        Ok(commands)
    }

    pub fn parse_file(&mut self, content: &str) -> EmbResult<Vec<GCodeCommand>> {
        let lines: Vec<&str> = content.lines().collect();
        self.parse_lines(&lines)
    }

    pub fn reset(&mut self) {
        self.line_number = 0;
    }

    pub fn current_line(&self) -> usize {
        self.line_number
    }
}

impl Default for GCodeParser {
    fn default() -> Self {
        Self::new()
    }
}