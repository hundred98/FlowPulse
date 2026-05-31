//! G-code Controller for 3D Printer Host
//!
//! This module provides high-level G-code command sending via Socket API.
//! All motion planning and serial communication is handled by emb-core-server.

use emb_public::{EmbError, EmbResult, gcode::{GCodeCommand, GCodeCategory, GCodeParser}};
use emb_public::CoreSocketClient;
use emb_api::ArcParamsApi;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;
use log::{info, warn, debug, error};

/// Pending motion segment tracking
#[derive(Debug, Clone)]
pub struct PendingSegment {
    pub gcode_source: String,
    pub segment_index: usize,
    pub total_segments: usize,
    pub sent_at: Instant,
    pub timeout_ms: u64,
    pub ack_received: bool,
}

impl PendingSegment {
    pub fn new(gcode_source: String, segment_index: usize, total_segments: usize) -> Self {
        Self {
            gcode_source,
            segment_index,
            total_segments,
            sent_at: Instant::now(),
            timeout_ms: 5000,
            ack_received: false,
        }
    }

    pub fn is_timed_out(&self) -> bool {
        !self.ack_received && self.sent_at.elapsed().as_millis() > self.timeout_ms as u128
    }
}

/// Printer status information
#[derive(Debug, Clone)]
pub struct PrinterStatus {
    pub hotend_temp: f32,
    pub bed_temp: f32,
    pub target_hotend_temp: f32,
    pub target_bed_temp: f32,
    pub x_pos: f32,
    pub y_pos: f32,
    pub z_pos: f32,
    pub e_pos: f32,
    pub state: String,
    pub credits: u8,
    pub last_update: Instant,
    pub is_ready: bool,
}

impl Default for PrinterStatus {
    fn default() -> Self {
        Self {
            hotend_temp: 0.0,
            bed_temp: 0.0,
            target_hotend_temp: 0.0,
            target_bed_temp: 0.0,
            x_pos: 0.0,
            y_pos: 0.0,
            z_pos: 0.0,
            e_pos: 0.0,
            state: "Unknown".to_string(),
            credits: 0,
            last_update: Instant::now(),
            is_ready: false,
        }
    }
}

/// G-code controller statistics
#[derive(Debug, Clone, Default)]
pub struct ControllerStats {
    pub gcode_commands: u64,
    pub segments_sent: u64,
    pub segments_acked: u64,
    pub segments_failed: u64,
    pub retries: u64,
    pub status_queries: u64,
    pub last_ack_time: Option<Instant>,
}

/// G-code controller configuration
#[derive(Debug, Clone)]
pub struct GCodeControllerConfig {
    pub command_timeout_ms: u64,
    pub status_poll_interval_ms: u64,
    pub max_retries: u32,
    pub enable_status_poll: bool,
    pub min_credits: u8,
}

impl Default for GCodeControllerConfig {
    fn default() -> Self {
        Self {
            command_timeout_ms: 5000,
            status_poll_interval_ms: 1000,
            max_retries: 3,
            enable_status_poll: true,
            min_credits: 4,
        }
    }
}

/// G-code controller using Socket API to communicate with emb-core-server
pub struct GCodeController {
    client: Arc<CoreSocketClient>,
    config: GCodeControllerConfig,
    pending_segments: Arc<tokio::sync::Mutex<Vec<PendingSegment>>>,
    status: Arc<tokio::sync::Mutex<PrinterStatus>>,
    stats: Arc<tokio::sync::Mutex<ControllerStats>>,
    running: Arc<tokio::sync::Mutex<bool>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl GCodeController {
    pub fn new(client: Arc<CoreSocketClient>, config: GCodeControllerConfig) -> Self {
        Self {
            client,
            config,
            pending_segments: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            status: Arc::new(tokio::sync::Mutex::new(PrinterStatus::default())),
            stats: Arc::new(tokio::sync::Mutex::new(ControllerStats::default())),
            running: Arc::new(tokio::sync::Mutex::new(false)),
            shutdown_tx: None,
        }
    }

    pub fn with_default_config(client: Arc<CoreSocketClient>) -> Self {
        Self::new(client, GCodeControllerConfig::default())
    }

    pub async fn start(&mut self) -> EmbResult<()> {
        if !self.client.is_connected().await {
            self.client.connect().await
                .map_err(|e| EmbError::Connection(format!("Socket connect failed: {}", e)))?;
        }
        *self.running.lock().await = true;
        info!("GCodeController started (Socket mode)");
        Ok(())
    }

    pub async fn stop(&mut self) -> EmbResult<()> {
        *self.running.lock().await = false;
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        self.client.disconnect().await;
        info!("GCodeController stopped");
        Ok(())
    }

    pub async fn send_gcode(&self, gcode: &GCodeCommand) -> EmbResult<String> {
        let command_str = gcode.to_string();
        let category = gcode.category();

        match category {
            GCodeCategory::LinearMove | GCodeCategory::RapidPositioning => {
                self.send_linear_move(gcode, &command_str).await
            }
            GCodeCategory::ArcCW | GCodeCategory::ArcCCW => {
                self.send_arc_move(gcode, &command_str).await
            }
            GCodeCategory::Home => {
                self.send_home_command(&command_str).await
            }
            GCodeCategory::SetPosition => {
                self.send_set_position(gcode, &command_str).await
            }
            _ => {
                Ok(format!("OK (queued: {})", command_str))
            }
        }
    }

    async fn send_linear_move(&self, gcode: &GCodeCommand, command_str: &str) -> EmbResult<String> {
        let x = gcode.get_param('X');
        let y = gcode.get_param('Y');
        let z = gcode.get_param('Z');
        let e = gcode.get_param('E');
        let f = gcode.get_param('F');

        let cmd_type = if gcode.code() == "G0" { "G0" } else { "G1" };

        let segments_dispatched = self.client.motion_dispatch(
            cmd_type,
            x, y, z, e, f,
        ).await.map_err(|e| EmbError::Motion(format!("Motion dispatch failed: {}", e)))?;

        if let Ok(mut s) = self.stats.lock().await.as_mut() {
            s.gcode_commands += 1;
            s.segments_sent += segments_dispatched as u64;
        }

        debug!("Dispatched {} segments for {}", segments_dispatched, command_str);
        Ok(format!("OK ({} segments dispatched)", segments_dispatched))
    }

    async fn send_arc_move(&self, gcode: &GCodeCommand, command_str: &str) -> EmbResult<String> {
        let x = gcode.get_param('X');
        let y = gcode.get_param('Y');
        let z = gcode.get_param('Z');
        let e = gcode.get_param('E');
        let f = gcode.get_param('F');
        let i = gcode.get_param('I').unwrap_or(0.0);
        let j = gcode.get_param('J').unwrap_or(0.0);

        let direction = if gcode.code() == "G2" { 0u8 } else { 1u8 };
        let arc_params = ArcParamsApi { i, j, direction };

        let cmd_type = if gcode.code() == "G2" { "G2" } else { "G3" };

        let segments_dispatched = self.client.motion_dispatch_arc(
            cmd_type,
            x, y, z, e, f,
            Some(arc_params),
        ).await.map_err(|e| EmbError::Motion(format!("Arc dispatch failed: {}", e)))?;

        if let Ok(mut s) = self.stats.lock().await.as_mut() {
            s.gcode_commands += 1;
            s.segments_sent += segments_dispatched as u64;
        }

        debug!("Dispatched {} arc segments for {}", segments_dispatched, command_str);
        Ok(format!("OK ({} arc segments dispatched)", segments_dispatched))
    }

    async fn send_home_command(&self, command_str: &str) -> EmbResult<String> {
        self.client.motion_reset_position().await
            .map_err(|e| EmbError::Motion(format!("Home failed: {}", e)))?;

        if let Ok(mut s) = self.stats.lock().await.as_mut() {
            s.gcode_commands += 1;
        }

        info!("Home command executed: {}", command_str);
        Ok("OK (home complete)".to_string())
    }

    async fn send_set_position(&self, gcode: &GCodeCommand, command_str: &str) -> EmbResult<String> {
        let x = gcode.get_param('X');
        let y = gcode.get_param('Y');
        let z = gcode.get_param('Z');
        let e = gcode.get_param('E');

        self.client.motion_set_position(x, y, z, e).await
            .map_err(|e| EmbError::Motion(format!("Set position failed: {}", e)))?;

        if let Ok(mut s) = self.stats.lock().await.as_mut() {
            s.gcode_commands += 1;
        }

        debug!("Set position: {}", command_str);
        Ok("OK (position set)".to_string())
    }

    pub async fn send_raw_gcode(&self, gcode: &str) -> EmbResult<String> {
        let mut parser = GCodeParser::new();
        let command = parser.parse_line(gcode)
            .map_err(|e| EmbError::GCodeParse(format!("Parse error: {:?}", e)))?
            .ok_or_else(|| EmbError::GCodeParse("Empty G-code line".to_string()))?;
        self.send_gcode(&command).await
    }

    pub async fn get_position(&self) -> EmbResult<(f32, f32, f32, f32)> {
        self.client.motion_get_position().await
            .map_err(|e| EmbError::Motion(format!("Get position failed: {}", e)))
    }

    pub async fn wait_motion_drain(&self) -> EmbResult<()> {
        self.client.motion_wait_drain().await
            .map_err(|e| EmbError::Motion(format!("Motion drain failed: {}", e)))
    }

    pub async fn get_status(&self) -> PrinterStatus {
        self.status.lock().await.clone()
    }

    pub async fn get_stats(&self) -> ControllerStats {
        self.stats.lock().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        *self.running.lock().await
    }

    pub async fn enter_print_mode(&self) -> EmbResult<()> {
        self.client.serial_enter_special_mode().await
            .map_err(|e| EmbError::Protocol(format!("Enter print mode failed: {}", e)))?;
        info!("Entered print mode (special mode)");
        Ok(())
    }

    pub async fn exit_print_mode(&self) -> EmbResult<()> {
        self.client.serial_exit_special_mode().await
            .map_err(|e| EmbError::Protocol(format!("Exit print mode failed: {}", e)))?;
        info!("Exited print mode");
        Ok(())
    }
}