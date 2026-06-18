//! G-code parsing module
//!
//! This module provides G-code parsing and processing functionality.

mod types;
mod parser;
mod converter;

// Re-export types
pub use types::{
    ParsedCommand, CommandKind, MotionCommand,
    MotionParams, AccelParams,
};

// Re-export parser
pub use parser::GCodeParser;

// Re-export converter
pub use converter::{
    MCommandConverter, DeviceCommand, ConvertError,
};

/// G-code file parser
///
/// Loads a .gcode file and provides metadata (total line count).
/// Actual line-by-line parsing is done by GCodeParser / execute_print_loop.
pub struct GCodeFileParser {
    /// File path
    file_path: Option<String>,

    /// Total lines in the file
    total_lines: u32,
}

impl GCodeFileParser {
    /// Create a new G-code file parser
    pub fn new() -> Self {
        Self {
            file_path: None,
            total_lines: 0,
        }
    }

    /// Load G-code file and count total lines
    pub fn load_file(&mut self, path: &str) -> crate::common::EmbResult<()> {
        let content = std::fs::read_to_string(path)?;
        self.total_lines = content.lines().count() as u32;
        self.file_path = Some(path.to_string());
        Ok(())
    }

    /// Get total lines
    pub fn total_lines(&self) -> u32 {
        self.total_lines
    }
}

impl Default for GCodeFileParser {
    fn default() -> Self {
        Self::new()
    }
}

