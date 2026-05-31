//! G-code Controller
//!
//! Provides G-code command queue and execution control.

use std::sync::{Arc, Mutex};

use crate::gcode::parser::GCodeCommand;
use crate::gcode::{GCodeParser, GCodeFileParser};
use crate::common::EmbResult;

#[derive(Debug, Clone)]
pub struct ControllerStats {
    pub total_commands: usize,
    pub executed_commands: usize,
    pub remaining_commands: usize,
    pub progress_percent: f32,
}

pub struct GCodeController {
    parser: GCodeParser,
    queue: Arc<Mutex<Vec<GCodeCommand>>>,
    current_index: Arc<Mutex<usize>>,
}

impl GCodeController {
    pub fn new() -> Self {
        Self {
            parser: GCodeParser::new(),
            queue: Arc::new(Mutex::new(Vec::new())),
            current_index: Arc::new(Mutex::new(0)),
        }
    }

    pub fn load_file(&self, file_path: &str) -> EmbResult<()> {
        let parser = GCodeFileParser::new(file_path)?;
        let commands = parser.parse_all()?;

        let mut queue = self.queue.lock().unwrap();
        *queue = commands;

        let mut index = self.current_index.lock().unwrap();
        *index = 0;

        Ok(())
    }

    pub fn load_commands(&self, commands: Vec<GCodeCommand>) {
        let mut queue = self.queue.lock().unwrap();
        *queue = commands;

        let mut index = self.current_index.lock().unwrap();
        *index = 0;
    }

    pub fn next_command(&self) -> Option<GCodeCommand> {
        let queue = self.queue.lock().unwrap();
        let mut index = self.current_index.lock().unwrap();

        if *index < queue.len() {
            let cmd = queue[*index].clone();
            *index += 1;
            Some(cmd)
        } else {
            None
        }
    }

    pub fn peek_command(&self) -> Option<GCodeCommand> {
        let queue = self.queue.lock().unwrap();
        let index = *self.current_index.lock().unwrap();
        queue.get(index).cloned()
    }

    pub fn remaining_count(&self) -> usize {
        let queue = self.queue.lock().unwrap();
        let index = *self.current_index.lock().unwrap();
        queue.len() - index
    }

    pub fn progress(&self) -> f32 {
        let queue = self.queue.lock().unwrap();
        let index = *self.current_index.lock().unwrap();

        if queue.is_empty() {
            return 0.0;
        }
        (index as f32 / queue.len() as f32) * 100.0
    }

    pub fn stats(&self) -> ControllerStats {
        let queue = self.queue.lock().unwrap();
        let index = *self.current_index.lock().unwrap();
        let total = queue.len();
        let executed = index;
        ControllerStats {
            total_commands: total,
            executed_commands: executed,
            remaining_commands: total.saturating_sub(executed),
            progress_percent: if total > 0 { (executed as f32 / total as f32) * 100.0 } else { 0.0 },
        }
    }

    pub fn reset(&self) {
        let mut index = self.current_index.lock().unwrap();
        *index = 0;
    }

    pub fn parse_line(&self, line: &str) -> Option<GCodeCommand> {
        let mut parser = GCodeParser::new();
        parser.parse_line(line).ok().flatten()
    }

    pub fn parse(&mut self, content: &str) -> EmbResult<Vec<GCodeCommand>> {
        self.parser.parse_file(content)
    }
}

impl Default for GCodeController {
    fn default() -> Self {
        Self::new()
    }
}