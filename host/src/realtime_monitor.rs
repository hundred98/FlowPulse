//! Real-time Monitoring System
//!
//! This module provides comprehensive real-time monitoring capabilities including:
//! - Temperature real-time acquisition and monitoring
//! - Position and velocity real-time monitoring
//! - Print process visualization
//! - Alert and alarm system
//! - WebSocket real-time data push
//!
//! All serial communication is handled via Socket API through emb-core-server.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use emb_public::{EmbError, CoreSocketClient};

/// Frame type constants (matching emb-core serial protocol)
pub const FRAME_TYPE_STATUS_QUERY: u8 = 0x03;
pub const FRAME_TYPE_STATUS_RESPONSE: u8 = 0x04;

/// Device status data parsed from StatusResponse frame
#[derive(Debug, Clone, Copy)]
pub struct DeviceStatusData {
    pub credits: u8,
    pub position_x: i32,
    pub position_y: i32,
    pub position_z: i32,
    pub position_e: i32,
    pub temp_bed: i16,
    pub temp_nozzle: i16,
    pub status: u8,
}

impl DeviceStatusData {
    pub fn from_payload(payload: &[u8]) -> Option<Self> {
        if payload.len() < 22 {
            return None;
        }
        Some(Self {
            credits: payload[0],
            position_x: i32::from_be_bytes([payload[1], payload[2], payload[3], payload[4]]),
            position_y: i32::from_be_bytes([payload[5], payload[6], payload[7], payload[8]]),
            position_z: i32::from_be_bytes([payload[9], payload[10], payload[11], payload[12]]),
            position_e: i32::from_be_bytes([payload[13], payload[14], payload[15], payload[16]]),
            temp_bed: i16::from_be_bytes([payload[17], payload[18]]),
            temp_nozzle: i16::from_be_bytes([payload[19], payload[20]]),
            status: payload[21],
        })
    }

    pub fn position_x_mm(&self) -> f32 {
        self.position_x as f32 / 1000.0
    }
    pub fn position_y_mm(&self) -> f32 {
        self.position_y as f32 / 1000.0
    }
    pub fn position_z_mm(&self) -> f32 {
        self.position_z as f32 / 1000.0
    }
    pub fn position_e_mm(&self) -> f32 {
        self.position_e as f32 / 1000.0
    }
    pub fn temp_bed_c(&self) -> f32 {
        self.temp_bed as f32 / 10.0
    }
    pub fn temp_nozzle_c(&self) -> f32 {
        self.temp_nozzle as f32 / 10.0
    }
}

/// Monitoring data types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitorDataType {
    Temperature,
    Position,
    Velocity,
    Progress,
    Status,
    Alert,
}

/// Temperature zone types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemperatureZone {
    Hotend,
    Bed,
    Chamber,
}

/// Temperature reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureReading {
    pub zone: TemperatureZone,
    pub current_temp: f32,
    pub target_temp: f32,
    pub power: f32,
    pub timestamp: DateTime<Utc>,
}

/// Position reading (X, Y, Z, E)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionReading {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub e: f32,
    pub timestamp: DateTime<Utc>,
}

/// Velocity reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelocityReading {
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    pub ve: f32,
    pub feed_rate: f32,
    pub timestamp: DateTime<Utc>,
}

/// Print progress data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressData {
    pub total_lines: usize,
    pub processed_lines: usize,
    pub progress_percent: f32,
    pub elapsed_time_ms: u64,
    pub estimated_remaining_ms: u64,
    pub current_layer: Option<u32>,
    pub total_layers: Option<u32>,
    pub timestamp: DateTime<Utc>,
}

/// Printer status snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSnapshot {
    pub state: String,
    pub is_printing: bool,
    pub is_paused: bool,
    pub has_error: bool,
    pub temperatures: Vec<TemperatureReading>,
    pub position: PositionReading,
    pub timestamp: DateTime<Utc>,
}

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

/// Alert/Alarm data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertData {
    pub id: String,
    pub severity: AlertSeverity,
    pub category: String,
    pub message: String,
    pub details: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
    pub resolved: bool,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub temp_sample_interval_ms: u64,
    pub position_sample_interval_ms: u64,
    pub progress_update_interval_ms: u64,
    pub status_snapshot_interval_ms: u64,
    pub max_history_size: usize,
    pub enable_temp_monitoring: bool,
    pub enable_position_monitoring: bool,
    pub enable_progress_monitoring: bool,
    pub temp_alert_thresholds: TemperatureAlertThresholds,
}

/// Temperature alert thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureAlertThresholds {
    pub hotend_max: f32,
    pub hotend_min: f32,
    pub bed_max: f32,
    pub bed_min: f32,
    pub deviation_threshold: f32,
    pub runaway_threshold: f32,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            temp_sample_interval_ms: 1000,
            position_sample_interval_ms: 100,
            progress_update_interval_ms: 500,
            status_snapshot_interval_ms: 1000,
            max_history_size: 3600,
            enable_temp_monitoring: true,
            enable_position_monitoring: true,
            enable_progress_monitoring: true,
            temp_alert_thresholds: TemperatureAlertThresholds::default(),
        }
    }
}

impl Default for TemperatureAlertThresholds {
    fn default() -> Self {
        Self {
            hotend_max: 280.0,
            hotend_min: 0.0,
            bed_max: 120.0,
            bed_min: 0.0,
            deviation_threshold: 10.0,
            runaway_threshold: 5.0,
        }
    }
}

/// Historical data container
#[derive(Debug, Clone)]
pub struct HistoricalData<T> {
    pub data: VecDeque<(DateTime<Utc>, T)>,
    pub max_size: usize,
}

impl<T: Clone> HistoricalData<T> {
    pub fn new(max_size: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    pub fn push(&mut self, timestamp: DateTime<Utc>, value: T) {
        if self.data.len() >= self.max_size {
            self.data.pop_front();
        }
        self.data.push_back((timestamp, value));
    }

    pub fn get_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<(DateTime<Utc>, T)> {
        self.data
            .iter()
            .filter(|(ts, _)| *ts >= start && *ts <= end)
            .cloned()
            .collect()
    }

    pub fn get_latest(&self) -> Option<(DateTime<Utc>, T)> {
        self.data.back().cloned()
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

/// Real-time monitoring manager using Socket API
pub struct RealtimeMonitor {
    config: Arc<RwLock<MonitoringConfig>>,
    
    temp_history: Arc<RwLock<HistoricalData<TemperatureReading>>>,
    position_history: Arc<RwLock<HistoricalData<PositionReading>>>,
    velocity_history: Arc<RwLock<HistoricalData<VelocityReading>>>,
    progress_history: Arc<RwLock<HistoricalData<ProgressData>>>,
    
    current_temperatures: Arc<RwLock<Vec<TemperatureReading>>>,
    current_position: Arc<RwLock<Option<PositionReading>>>,
    current_velocity: Arc<RwLock<Option<VelocityReading>>>,
    current_progress: Arc<RwLock<Option<ProgressData>>>,
    
    active_alerts: Arc<RwLock<Vec<AlertData>>>,
    alert_history: Arc<RwLock<HistoricalData<AlertData>>>,
    
    temp_tx: broadcast::Sender<TemperatureReading>,
    position_tx: broadcast::Sender<PositionReading>,
    velocity_tx: broadcast::Sender<VelocityReading>,
    progress_tx: broadcast::Sender<ProgressData>,
    status_tx: broadcast::Sender<StatusSnapshot>,
    alert_tx: broadcast::Sender<AlertData>,
    
    is_running: Arc<AtomicBool>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl RealtimeMonitor {
    pub fn new(config: MonitoringConfig) -> Self {
        let (temp_tx, _) = broadcast::channel(100);
        let (position_tx, _) = broadcast::channel(100);
        let (velocity_tx, _) = broadcast::channel(100);
        let (progress_tx, _) = broadcast::channel(100);
        let (status_tx, _) = broadcast::channel(100);
        let (alert_tx, _) = broadcast::channel(100);
        
        let max_history = config.max_history_size;
        
        Self {
            config: Arc::new(RwLock::new(config)),
            temp_history: Arc::new(RwLock::new(HistoricalData::new(max_history))),
            position_history: Arc::new(RwLock::new(HistoricalData::new(max_history))),
            velocity_history: Arc::new(RwLock::new(HistoricalData::new(max_history))),
            progress_history: Arc::new(RwLock::new(HistoricalData::new(max_history))),
            current_temperatures: Arc::new(RwLock::new(Vec::new())),
            current_position: Arc::new(RwLock::new(None)),
            current_velocity: Arc::new(RwLock::new(None)),
            current_progress: Arc::new(RwLock::new(None)),
            active_alerts: Arc::new(RwLock::new(Vec::new())),
            alert_history: Arc::new(RwLock::new(HistoricalData::new(1000))),
            temp_tx,
            position_tx,
            velocity_tx,
            progress_tx,
            status_tx,
            alert_tx,
            is_running: Arc::new(AtomicBool::new(false)),
            shutdown_tx: None,
        }
    }

    pub async fn start(&mut self) -> Result<(), EmbError> {
        if self.is_running.load(Ordering::Relaxed) {
            return Err(EmbError::StateMachine("Monitor already running".to_string()));
        }

        self.is_running.store(true, Ordering::Relaxed);
        
        let (shutdown_tx, _shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        if self.config.read().await.enable_temp_monitoring {
            let temp_interval = self.config.read().await.temp_sample_interval_ms;
            let is_running = Arc::clone(&self.is_running);
            let temp_history = Arc::clone(&self.temp_history);
            let current_temps = Arc::clone(&self.current_temperatures);
            let temp_tx = self.temp_tx.clone();
            let alert_tx = self.alert_tx.clone();
            let thresholds = self.config.read().await.temp_alert_thresholds.clone();
            
            tokio::spawn(async move {
                while is_running.load(Ordering::Relaxed) {
                    let reading = Self::simulate_temperature_reading();
                    
                    {
                        let mut temps = current_temps.write().await;
                        if let Some(existing) = temps.iter_mut().find(|t| t.zone == reading.zone) {
                            *existing = reading.clone();
                        } else {
                            temps.push(reading.clone());
                        }
                    }
                    
                    {
                        let mut history = temp_history.write().await;
                        history.push(reading.timestamp, reading.clone());
                    }
                    
                    let _ = temp_tx.send(reading.clone());
                    
                    Self::check_temperature_alerts(&reading, &thresholds, &alert_tx).await;
                    
                    tokio::time::sleep(Duration::from_millis(temp_interval)).await;
                }
            });
        }

        if self.config.read().await.enable_position_monitoring {
            let position_interval = self.config.read().await.position_sample_interval_ms;
            let is_running = Arc::clone(&self.is_running);
            let position_history = Arc::clone(&self.position_history);
            let velocity_history = Arc::clone(&self.velocity_history);
            let current_pos = Arc::clone(&self.current_position);
            let current_vel = Arc::clone(&self.current_velocity);
            let position_tx = self.position_tx.clone();
            let velocity_tx = self.velocity_tx.clone();
            
            tokio::spawn(async move {
                let mut last_position: Option<PositionReading> = None;
                let mut last_time = Instant::now();
                
                while is_running.load(Ordering::Relaxed) {
                    let position = Self::simulate_position_reading();
                    let now = Instant::now();
                    
                    {
                        let mut pos = current_pos.write().await;
                        *pos = Some(position.clone());
                    }
                    
                    {
                        let mut history = position_history.write().await;
                        history.push(position.timestamp, position.clone());
                    }
                    
                    if let Some(ref last) = last_position {
                        let dt = now.duration_since(last_time).as_secs_f32();
                        if dt > 0.0 {
                            let velocity = VelocityReading {
                                vx: (position.x - last.x) / dt,
                                vy: (position.y - last.y) / dt,
                                vz: (position.z - last.z) / dt,
                                ve: (position.e - last.e) / dt,
                                feed_rate: ((position.x - last.x).powi(2) + 
                                         (position.y - last.y).powi(2) + 
                                         (position.z - last.z).powi(2)).sqrt() / dt * 60.0,
                                timestamp: position.timestamp,
                            };
                            
                            {
                                let mut vel = current_vel.write().await;
                                *vel = Some(velocity.clone());
                            }
                            
                            {
                                let mut history = velocity_history.write().await;
                                history.push(velocity.timestamp, velocity.clone());
                            }
                            
                            let _ = velocity_tx.send(velocity);
                        }
                    }
                    
                    let _ = position_tx.send(position.clone());
                    
                    last_position = Some(position);
                    last_time = now;
                    
                    tokio::time::sleep(Duration::from_millis(position_interval)).await;
                }
            });
        }

        if self.config.read().await.enable_progress_monitoring {
            let progress_interval = self.config.read().await.progress_update_interval_ms;
            let is_running = Arc::clone(&self.is_running);
            let progress_history = Arc::clone(&self.progress_history);
            let current_progress = Arc::clone(&self.current_progress);
            let progress_tx = self.progress_tx.clone();
            
            tokio::spawn(async move {
                while is_running.load(Ordering::Relaxed) {
                    let progress = Self::simulate_progress_data();
                    
                    {
                        let mut prog = current_progress.write().await;
                        *prog = Some(progress.clone());
                    }
                    
                    {
                        let mut history = progress_history.write().await;
                        history.push(progress.timestamp, progress.clone());
                    }
                    
                    let _ = progress_tx.send(progress);
                    
                    tokio::time::sleep(Duration::from_millis(progress_interval)).await;
                }
            });
        }

        let status_interval = self.config.read().await.status_snapshot_interval_ms;
        let is_running = Arc::clone(&self.is_running);
        let current_temps = Arc::clone(&self.current_temperatures);
        let current_pos = Arc::clone(&self.current_position);
        let status_tx = self.status_tx.clone();
        
        tokio::spawn(async move {
            while is_running.load(Ordering::Relaxed) {
                let temps = current_temps.read().await.clone();
                let position = current_pos.read().await.clone().unwrap_or(PositionReading {
                    x: 0.0, y: 0.0, z: 0.0, e: 0.0,
                    timestamp: Utc::now(),
                });
                
                let status = StatusSnapshot {
                    state: "Printing".to_string(),
                    is_printing: true,
                    is_paused: false,
                    has_error: false,
                    temperatures: temps,
                    position,
                    timestamp: Utc::now(),
                };
                
                let _ = status_tx.send(status);
                
                tokio::time::sleep(Duration::from_millis(status_interval)).await;
            }
        });

        Ok(())
    }

    pub async fn start_with_client(&mut self, client: Arc<CoreSocketClient>) -> Result<(), EmbError> {
        if self.is_running.load(Ordering::Relaxed) {
            return Err(EmbError::StateMachine("Monitor already running".to_string()));
        }

        self.is_running.store(true, Ordering::Relaxed);

        let (shutdown_tx, _shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let query_interval = self.config.read().await.position_sample_interval_ms;
        let is_running = Arc::clone(&self.is_running);
        let position_history = Arc::clone(&self.position_history);
        let velocity_history = Arc::clone(&self.velocity_history);
        let current_pos = Arc::clone(&self.current_position);
        let current_vel = Arc::clone(&self.current_velocity);
        let current_temps = Arc::clone(&self.current_temperatures);
        let temp_history = Arc::clone(&self.temp_history);
        let temp_tx = self.temp_tx.clone();
        let position_tx = self.position_tx.clone();
        let velocity_tx = self.velocity_tx.clone();
        let status_tx = self.status_tx.clone();
        let alert_tx = self.alert_tx.clone();
        let thresholds = self.config.read().await.temp_alert_thresholds.clone();

        tokio::spawn(async move {
            let mut last_position: Option<PositionReading> = None;
            let mut last_time = Instant::now();

            while is_running.load(Ordering::Relaxed) {
                let status_query_payload: Vec<u8> = Vec::new();
                
                if client.serial_send_frame(FRAME_TYPE_STATUS_QUERY, status_query_payload).await.is_err() {
                    tokio::time::sleep(Duration::from_millis(query_interval)).await;
                    continue;
                }

                let wait_start = Instant::now();
                let timeout = Duration::from_millis(500);

                while wait_start.elapsed() < timeout {
                    if let Some((frame_type, payload)) = client.serial_recv_frame().await.ok().flatten() {
                        if frame_type == FRAME_TYPE_STATUS_RESPONSE {
                            if let Some(status) = DeviceStatusData::from_payload(&payload) {
                                let now = Utc::now();

                                let position = PositionReading {
                                    x: status.position_x_mm(),
                                    y: status.position_y_mm(),
                                    z: status.position_z_mm(),
                                    e: status.position_e_mm(),
                                    timestamp: now,
                                };

                                {
                                    let mut pos = current_pos.write().await;
                                    *pos = Some(position.clone());
                                }
                                {
                                    let mut history = position_history.write().await;
                                    history.push(now, position.clone());
                                }

                                let hotend_temp = TemperatureReading {
                                    zone: TemperatureZone::Hotend,
                                    current_temp: status.temp_nozzle_c(),
                                    target_temp: 0.0,
                                    power: 0.0,
                                    timestamp: now,
                                };
                                let bed_temp = TemperatureReading {
                                    zone: TemperatureZone::Bed,
                                    current_temp: status.temp_bed_c(),
                                    target_temp: 0.0,
                                    power: 0.0,
                                    timestamp: now,
                                };

                                {
                                    let mut temps = current_temps.write().await;
                                    temps.clear();
                                    temps.push(hotend_temp.clone());
                                    temps.push(bed_temp.clone());
                                }
                                {
                                    let mut history = temp_history.write().await;
                                    history.push(now, hotend_temp.clone());
                                    history.push(now, bed_temp.clone());
                                }

                                let _ = temp_tx.send(hotend_temp);
                                let _ = temp_tx.send(bed_temp);

                                Self::check_temperature_alerts(&hotend_temp, &thresholds, &alert_tx).await;
                                Self::check_temperature_alerts(&bed_temp, &thresholds, &alert_tx).await;

                                let instant_now = Instant::now();
                                if let Some(ref last) = last_position {
                                    let dt = instant_now.duration_since(last_time).as_secs_f32();
                                    if dt > 0.0 {
                                        let velocity = VelocityReading {
                                            vx: (position.x - last.x) / dt,
                                            vy: (position.y - last.y) / dt,
                                            vz: (position.z - last.z) / dt,
                                            ve: (position.e - last.e) / dt,
                                            feed_rate: ((position.x - last.x).powi(2)
                                                + (position.y - last.y).powi(2)
                                                + (position.z - last.z).powi(2))
                                            .sqrt()
                                                / dt
                                                * 60.0,
                                            timestamp: now,
                                        };
                                        {
                                            let mut vel = current_vel.write().await;
                                            *vel = Some(velocity.clone());
                                        }
                                        {
                                            let mut history = velocity_history.write().await;
                                            history.push(now, velocity.clone());
                                        }
                                        let _ = velocity_tx.send(velocity);
                                    }
                                }

                                let _ = position_tx.send(position.clone());

                                last_position = Some(position);
                                last_time = instant_now;
                                break;
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }

                tokio::time::sleep(Duration::from_millis(query_interval)).await;
            }
        });

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), EmbError> {
        self.is_running.store(false, Ordering::Relaxed);
        
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        
        Ok(())
    }

    fn simulate_temperature_reading() -> TemperatureReading {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        
        TemperatureReading {
            zone: TemperatureZone::Hotend,
            current_temp: 200.0 + rng.gen_range(-5.0..5.0),
            target_temp: 200.0,
            power: rng.gen_range(0.5..1.0),
            timestamp: Utc::now(),
        }
    }

    fn simulate_position_reading() -> PositionReading {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        
        PositionReading {
            x: rng.gen_range(0.0..200.0),
            y: rng.gen_range(0.0..200.0),
            z: rng.gen_range(0.0..50.0),
            e: rng.gen_range(0.0..100.0),
            timestamp: Utc::now(),
        }
    }

    fn simulate_progress_data() -> ProgressData {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        
        let total_lines = 1000;
        let processed_lines = rng.gen_range(0..total_lines);
        
        ProgressData {
            total_lines,
            processed_lines,
            progress_percent: processed_lines as f32 / total_lines as f32 * 100.0,
            elapsed_time_ms: processed_lines as u64 * 100,
            estimated_remaining_ms: (total_lines - processed_lines) as u64 * 100,
            current_layer: Some(processed_lines as u32 / 50),
            total_layers: Some(20),
            timestamp: Utc::now(),
        }
    }

    async fn check_temperature_alerts(
        reading: &TemperatureReading,
        thresholds: &TemperatureAlertThresholds,
        alert_tx: &broadcast::Sender<AlertData>,
    ) {
        let (max_threshold, min_threshold) = match reading.zone {
            TemperatureZone::Hotend => (thresholds.hotend_max, thresholds.hotend_min),
            TemperatureZone::Bed => (thresholds.bed_max, thresholds.bed_min),
            TemperatureZone::Chamber => (80.0, 0.0),
        };

        if reading.current_temp > max_threshold {
            let alert = AlertData {
                id: format!("temp_high_{}_{}", reading.zone, Utc::now().timestamp()),
                severity: AlertSeverity::Critical,
                category: "Temperature".to_string(),
                message: format!("{} temperature too high: {:.1}°C", 
                    match reading.zone {
                        TemperatureZone::Hotend => "Hotend",
                        TemperatureZone::Bed => "Bed",
                        TemperatureZone::Chamber => "Chamber",
                    },
                    reading.current_temp),
                details: Some(format!("Maximum threshold: {:.1}°C", max_threshold)),
                timestamp: Utc::now(),
                acknowledged: false,
                resolved: false,
            };
            let _ = alert_tx.send(alert);
        }

        if reading.current_temp < min_threshold && reading.target_temp > 0.0 {
            let alert = AlertData {
                id: format!("temp_low_{}_{}", reading.zone, Utc::now().timestamp()),
                severity: AlertSeverity::Warning,
                category: "Temperature".to_string(),
                message: format!("{} temperature below target: {:.1}°C (target: {:.1}°C)",
                    match reading.zone {
                        TemperatureZone::Hotend => "Hotend",
                        TemperatureZone::Bed => "Bed",
                        TemperatureZone::Chamber => "Chamber",
                    },
                    reading.current_temp, reading.target_temp),
                details: None,
                timestamp: Utc::now(),
                acknowledged: false,
                resolved: false,
            };
            let _ = alert_tx.send(alert);
        }
    }

    pub fn subscribe_temperature(&self) -> broadcast::Receiver<TemperatureReading> {
        self.temp_tx.subscribe()
    }

    pub fn subscribe_position(&self) -> broadcast::Receiver<PositionReading> {
        self.position_tx.subscribe()
    }

    pub fn subscribe_velocity(&self) -> broadcast::Receiver<VelocityReading> {
        self.velocity_tx.subscribe()
    }

    pub fn subscribe_progress(&self) -> broadcast::Receiver<ProgressData> {
        self.progress_tx.subscribe()
    }

    pub fn subscribe_status(&self) -> broadcast::Receiver<StatusSnapshot> {
        self.status_tx.subscribe()
    }

    pub fn subscribe_alerts(&self) -> broadcast::Receiver<AlertData> {
        self.alert_tx.subscribe()
    }

    pub async fn get_current_temperatures(&self) -> Vec<TemperatureReading> {
        self.current_temperatures.read().await.clone()
    }

    pub async fn get_current_position(&self) -> Option<PositionReading> {
        self.current_position.read().await.clone()
    }

    pub async fn get_current_velocity(&self) -> Option<VelocityReading> {
        self.current_velocity.read().await.clone()
    }

    pub async fn get_current_progress(&self) -> Option<ProgressData> {
        self.current_progress.read().await.clone()
    }

    pub async fn get_temperature_history(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<(DateTime<Utc>, TemperatureReading)> {
        self.temp_history.read().await.get_range(start, end)
    }

    pub async fn get_position_history(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<(DateTime<Utc>, PositionReading)> {
        self.position_history.read().await.get_range(start, end)
    }

    pub async fn get_velocity_history(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<(DateTime<Utc>, VelocityReading)> {
        self.velocity_history.read().await.get_range(start, end)
    }

    pub async fn get_progress_history(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<(DateTime<Utc>, ProgressData)> {
        self.progress_history.read().await.get_range(start, end)
    }

    pub async fn get_active_alerts(&self) -> Vec<AlertData> {
        self.active_alerts.read().await.clone()
    }

    pub async fn acknowledge_alert(&self, alert_id: &str) -> Result<(), EmbError> {
        let mut alerts = self.active_alerts.write().await;
        if let Some(alert) = alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.acknowledged = true;
            Ok(())
        } else {
            Err(EmbError::NotFound(format!("Alert not found: {}", alert_id)))
        }
    }

    pub async fn resolve_alert(&self, alert_id: &str) -> Result<(), EmbError> {
        let mut alerts = self.active_alerts.write().await;
        let alert = alerts.iter_mut().find(|a| a.id == alert_id);
        if let Some(alert) = alert {
            alert.resolved = true;
            Ok(())
        } else {
            Err(EmbError::NotFound(format!("Alert not found: {}", alert_id)))
        }
    }

    pub async fn clear_history(&self) {
        self.temp_history.write().await.clear();
        self.position_history.write().await.clear();
        self.velocity_history.write().await.clear();
        self.progress_history.write().await.clear();
        self.alert_history.write().await.clear();
    }

    pub async fn update_config(&self, config: MonitoringConfig) {
        *self.config.write().await = config;
    }

    pub async fn get_config(&self) -> MonitoringConfig {
        self.config.read().await.clone()
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }
}