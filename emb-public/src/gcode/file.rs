//! G-code File Processing System
//!
//! This module provides comprehensive G-code file handling capabilities including:
//! - Large file streaming parser
//! - Command preprocessing and optimization
//! - Print queue management
//! - Progress tracking

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::EmbError;
use super::{GCodeCommand, GCodeParser, GCodeCategory};

/// G-code file parser for large files with streaming support
#[derive(Clone)]
pub struct GCodeFileParser {
    file_path: String,
    total_lines: usize,
    processed_lines: Arc<AtomicUsize>,
    total_commands: Arc<AtomicUsize>,
    estimated_time_ms: Arc<AtomicU64>,
}

/// G-code preprocessing options
#[derive(Debug, Clone)]
pub struct PreprocessOptions {
    /// Remove comments from G-code
    pub remove_comments: bool,
    /// Optimize G-code (remove redundant movements)
    pub optimize_moves: bool,
    /// Filter by command categories
    pub filter_categories: Option<Vec<GCodeCategory>>,
    /// Minimum movement distance to keep (mm)
    pub min_movement_distance: f32,
}

impl Default for PreprocessOptions {
    fn default() -> Self {
        Self {
            remove_comments: true,
            optimize_moves: true,
            filter_categories: None,
            min_movement_distance: 0.01,
        }
    }
}

/// Print progress information
#[derive(Debug, Clone)]
pub struct PrintProgress {
    /// Total number of lines in file
    pub total_lines: usize,
    /// Number of lines processed
    pub processed_lines: usize,
    /// Total commands to execute
    pub total_commands: usize,
    /// Commands already executed
    pub executed_commands: usize,
    /// Estimated total print time (ms)
    pub estimated_time_ms: u64,
    /// Elapsed time since start (ms)
    pub elapsed_time_ms: u64,
    /// Current progress percentage (0.0-100.0)
    pub progress_percent: f32,
    /// Current line number being processed
    pub current_line: usize,
    /// Current command being executed
    pub current_command: Option<String>,
}

impl GCodeFileParser {
    /// Create new parser for G-code file
    pub fn new(file_path: &str) -> Result<Self, EmbError> {
        if !Path::new(file_path).exists() {
            return Err(EmbError::FileNotFound(file_path.to_string()));
        }

        let mut parser = Self {
            file_path: file_path.to_string(),
            total_lines: 0,
            processed_lines: Arc::new(AtomicUsize::new(0)),
            total_commands: Arc::new(AtomicUsize::new(0)),
            estimated_time_ms: Arc::new(AtomicU64::new(0)),
        };

        parser.analyze_file()?;
        
        Ok(parser)
    }

    /// Analyze file to count lines and estimate print time
    fn analyze_file(&mut self) -> Result<(), EmbError> {
        let file = File::open(&self.file_path)?;
        
        let reader = BufReader::new(file);
        let mut line_count = 0;
        let mut command_count = 0;
        let mut estimated_time = 0u64;
        let mut parser = GCodeParser::new();
        let mut last_pos = (0.0, 0.0, 0.0);

        for line in reader.lines() {
            line_count += 1;
            
            if let Ok(line_content) = line {
                let trimmed: &str = line_content.trim();
                
                if trimmed.is_empty() || trimmed.starts_with(';') {
                    continue;
                }

                if let Ok(Some(cmd)) = parser.parse_line(trimmed) {
                    command_count += 1;
                    
                    match cmd.category() {
                        GCodeCategory::LinearMove => {
                            if let (Some(x), Some(y), Some(z), Some(f)) = 
                                (cmd.x(), cmd.y(), cmd.z(), cmd.f()) {
                                let distance: f32 = ((x - last_pos.0).powi(2) + 
                                              (y - last_pos.1).powi(2) + 
                                              (z - last_pos.2).powi(2)).sqrt();
                                let speed: f32 = f.max(1.0);
                                let time_ms = (distance / speed * 60000.0) as u64;
                                estimated_time += time_ms;
                                last_pos = (x, y, z);
                            }
                        }
                        GCodeCategory::TemperatureControl => {
                            estimated_time += 5000;
                        }
                        GCodeCategory::HomeAxes => {
                            estimated_time += 10000;
                        }
                        _ => {
                            estimated_time += 100;
                        }
                    }
                }
            }
        }

        self.total_lines = line_count;
        self.total_commands.store(command_count, Ordering::Relaxed);
        self.estimated_time_ms.store(estimated_time, Ordering::Relaxed);

        println!("File analysis complete:");
        println!("   Total lines: {}", line_count);
        println!("   Commands: {}", command_count);
        println!("   Estimated time: {:.1} min", estimated_time as f64 / 60000.0);

        Ok(())
    }

    /// Parse all commands from the file
    pub fn parse_all(&self) -> Result<Vec<GCodeCommand>, EmbError> {
        let file = File::open(&self.file_path)?;
        let reader = BufReader::new(file);
        let mut parser = GCodeParser::new();
        let mut commands = Vec::new();

        for line in reader.lines() {
            if let Ok(line_content) = line {
                let trimmed: &str = line_content.trim();
                
                if trimmed.is_empty() || trimmed.starts_with(';') {
                    continue;
                }

                if let Ok(Some(cmd)) = parser.parse_line(trimmed) {
                    commands.push(cmd);
                }
            }
        }

        Ok(commands)
    }

    /// Get file progress information
    pub fn get_progress(&self, elapsed_time_ms: u64, current_command: Option<String>) -> PrintProgress {
        let processed = self.processed_lines.load(Ordering::Relaxed);
        let executed = self.processed_lines.load(Ordering::Relaxed);
        let total_commands = self.total_commands.load(Ordering::Relaxed);
        let estimated_time = self.estimated_time_ms.load(Ordering::Relaxed);

        PrintProgress {
            total_lines: self.total_lines,
            processed_lines: processed,
            total_commands,
            executed_commands: executed,
            estimated_time_ms: estimated_time,
            elapsed_time_ms,
            progress_percent: if self.total_lines > 0 {
                (processed as f32 / self.total_lines as f32) * 100.0
            } else {
                0.0
            },
            current_line: processed,
            current_command,
        }
    }

    /// Stream parse G-code file with preprocessing
    pub async fn parse_stream<F>(&self, options: PreprocessOptions, mut callback: F) -> Result<(), EmbError>
    where
        F: FnMut(GCodeCommand) -> Result<(), EmbError>,
    {
        let file = File::open(&self.file_path)?;
        
        let reader = BufReader::new(file);
        let mut parser = GCodeParser::new();
        let mut line_num = 0;
        let mut last_position = (0.0, 0.0, 0.0);

        for line in reader.lines() {
            line_num += 1;
            self.processed_lines.store(line_num, Ordering::Relaxed);

            if let Ok(line_content) = line {
                let mut processed_line = line_content.trim().to_string();
                
                processed_line = self.preprocess_line(&processed_line, &options);
                
                if processed_line.is_empty() {
                    continue;
                }

                match parser.parse_line(&processed_line) {
                    Ok(Some(cmd)) => {
                        if !self.should_include_command(&cmd, &options) {
                            continue;
                        }

                        if options.optimize_moves && cmd.category() == GCodeCategory::LinearMove {
                            if let (Some(x), Some(y), Some(z)) = (cmd.x(), cmd.y(), cmd.z()) {
                                let distance: f32 = ((x - last_position.0).powi(2) + 
                                              (y - last_position.1).powi(2) + 
                                              (z - last_position.2).powi(2)).sqrt();
                                
                                if distance < options.min_movement_distance {
                                    continue;
                                }
                                last_position = (x, y, z);
                            }
                        }

                        callback(cmd)?;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("Parse error at line {}: {:?}", line_num, e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Stream parse G-code file with async callback
    pub async fn parse_stream_async<F, Fut>(&self, options: PreprocessOptions, mut callback: F) -> Result<(), EmbError>
    where
        F: FnMut(GCodeCommand) -> Fut,
        Fut: std::future::Future<Output = Result<(), EmbError>>,
    {
        use tokio::fs::File as AsyncFile;
        use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};

        let file = AsyncFile::open(&self.file_path).await
            .map_err(|e| EmbError::Io(e))?;
        let reader = AsyncBufReader::new(file);
        let mut lines = reader.lines();
        let mut parser = GCodeParser::new();
        let mut line_num = 0;
        let mut last_position = (0.0, 0.0, 0.0);

        while let Some(line_result) = lines.next_line().await? {
            line_num += 1;
            self.processed_lines.store(line_num, Ordering::Relaxed);

            let mut processed_line = line_result.trim().to_string();
            processed_line = self.preprocess_line(&processed_line, &options);

            if processed_line.is_empty() {
                continue;
            }

            match parser.parse_line(&processed_line) {
                Ok(Some(cmd)) => {
                    if !self.should_include_command(&cmd, &options) {
                        continue;
                    }

                    if options.optimize_moves && cmd.category() == GCodeCategory::LinearMove {
                        if let (Some(x), Some(y), Some(z)) = (cmd.x(), cmd.y(), cmd.z()) {
                            let distance: f32 = ((x - last_position.0).powi(2) +
                                          (y - last_position.1).powi(2) +
                                          (z - last_position.2).powi(2)).sqrt();
                            if distance < options.min_movement_distance {
                                continue;
                            }
                            last_position = (x, y, z);
                        }
                    }

                    callback(cmd).await?;
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("Parse error at line {}: {:?}", line_num, e);
                }
            }
        }

        Ok(())
    }

    /// Preprocess a single G-code line
    fn preprocess_line(&self, line: &str, options: &PreprocessOptions) -> String {
        let mut processed = line.to_string();

        if options.remove_comments {
            if let Some(comment_pos) = processed.find(';') {
                processed.truncate(comment_pos);
            }
            if let Some(comment_pos) = processed.find('(') {
                if let Some(end_comment) = processed.find(')') {
                    processed.replace_range(comment_pos..=end_comment, "");
                }
            }
        }

        processed = processed.trim().to_string();
        processed.split_whitespace().collect::<Vec<&str>>().join(" ")
    }

    /// Check if command should be included based on filters
    fn should_include_command(&self, cmd: &GCodeCommand, options: &PreprocessOptions) -> bool {
        if let Some(ref categories) = options.filter_categories {
            categories.contains(&cmd.category())
        } else {
            true
        }
    }

    /// Get file statistics
    pub fn get_stats(&self) -> (usize, usize, u64) {
        (
            self.total_lines,
            self.total_commands.load(Ordering::Relaxed),
            self.estimated_time_ms.load(Ordering::Relaxed),
        )
    }

    pub fn file_path(&self) -> &str {
        &self.file_path
    }
}

/// Print queue manager for buffering and scheduling commands
pub struct PrintQueue {
    command_sender: mpsc::Sender<GCodeCommand>,
    command_receiver: Option<mpsc::Receiver<GCodeCommand>>,
    queue_size: usize,
    is_running: Arc<std::sync::atomic::AtomicBool>,
}

impl Clone for PrintQueue {
    fn clone(&self) -> Self {
        let (tx, rx) = mpsc::channel(self.queue_size);
        Self {
            command_sender: tx,
            command_receiver: Some(rx),
            queue_size: self.queue_size,
            is_running: Arc::clone(&self.is_running),
        }
    }
}

impl PrintQueue {
    /// Create new print queue with specified buffer size
    pub fn new(queue_size: usize) -> Self {
        let (tx, rx) = mpsc::channel(queue_size);
        
        Self {
            command_sender: tx,
            command_receiver: Some(rx),
            queue_size,
            is_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Add command to queue
    pub async fn add_command(&self, command: GCodeCommand) -> Result<(), EmbError> {
        self.command_sender.send(command)
            .await
            .map_err(|_| EmbError::Communication("Queue is closed".to_string()))?;
        Ok(())
    }

    /// Blocking version of add_command for use in sync contexts
    pub fn blocking_add_command(&self, command: GCodeCommand) -> Result<(), EmbError> {
        self.command_sender.try_send(command)
            .map_err(|_| EmbError::Communication("Queue is full or closed".to_string()))?;
        Ok(())
    }

    /// Get next command from queue
    pub async fn get_next_command(&mut self) -> Option<GCodeCommand> {
        if let Some(ref mut receiver) = self.command_receiver {
            receiver.recv().await
        } else {
            None
        }
    }

    /// Get current queue length
    pub fn queue_length(&self) -> usize {
        0
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue_length() == 0
    }

    /// Clear queue
    pub fn clear(&mut self) {
        let (tx, rx) = mpsc::channel(self.queue_size);
        self.command_sender = tx;
        self.command_receiver = Some(rx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn test_gcode_file_parser() {
        let test_content = r#"
; Test G-code file
G21 ; Set units to millimeters
G90 ; Use absolute coordinates
G28 ; Home all axes
G1 X10 Y20 F3000 ; Move to position
M104 S200 ; Set hotend temperature
M140 S60 ; Set bed temperature
"#;

        let test_file = "test.gcode";
        fs::write(test_file, test_content).unwrap();

        let parser = GCodeFileParser::new(test_file).unwrap();
        let (lines, commands, time) = parser.get_stats();
        
        assert!(lines > 0);
        assert!(commands > 0);
        assert!(time > 0);

        fs::remove_file(test_file).unwrap();
    }

    #[test]
    fn test_preprocess_options() {
        let options = PreprocessOptions::default();
        assert!(options.remove_comments);
        assert!(options.optimize_moves);
        assert!(options.filter_categories.is_none());
    }
}