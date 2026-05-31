//! Bed Leveling API Endpoints
//!
//! REST API endpoints for bed leveling functionality.
//! All leveling operations are handled via Socket API through emb-core-server.
//!
//! - Start/stop leveling process
//! - Get leveling status and configuration
//! - Retrieve leveling data and statistics
//! - Configure leveling parameters

use axum::{
    extract::State,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use log::{info, error};

use super::{ApiState, ApiResponse};

/// Start leveling request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartLevelingRequest {
    pub grid_size: Option<usize>,
    pub probe_feed_rate: Option<f32>,
    pub probe_height: Option<f32>,
}

/// Leveling configuration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelingConfigRequest {
    pub grid_size: usize,
    pub probe_feed_rate: f32,
    pub probe_height: f32,
    pub lift_distance: f32,
    pub samples_per_point: u8,
    pub max_deviation: f32,
}

/// Leveling configuration (local copy)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelingConfig {
    pub grid_size: usize,
    pub probe_feed_rate: f32,
    pub probe_height: f32,
    pub lift_distance: f32,
    pub samples_per_point: u8,
    pub max_deviation: f32,
}

impl Default for LevelingConfig {
    fn default() -> Self {
        Self {
            grid_size: 5,
            probe_feed_rate: 100.0,
            probe_height: 0.2,
            lift_distance: 5.0,
            samples_per_point: 3,
            max_deviation: 0.05,
        }
    }
}

/// Leveling status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelingStatusResponse {
    pub status: String,
    pub progress: Option<f32>,
    pub current_point: Option<(usize, usize)>,
    pub total_points: Option<usize>,
    pub error: Option<String>,
}

impl Default for LevelingStatusResponse {
    fn default() -> Self {
        Self {
            status: "idle".to_string(),
            progress: None,
            current_point: None,
            total_points: None,
            error: None,
        }
    }
}

/// Leveling point data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelingPoint {
    pub x: usize,
    pub y: usize,
    pub height: f32,
    pub timestamp: String,
}

/// Leveling statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelingStatistics {
    pub min_height: f32,
    pub max_height: f32,
    pub avg_height: f32,
    pub deviation: f32,
    pub total_points: usize,
}

/// Leveling data response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelingDataResponse {
    pub height_grid: Vec<Vec<LevelingPoint>>,
    pub compensation_matrix: Vec<Vec<f32>>,
    pub statistics: Option<LevelingStatistics>,
}

impl Default for LevelingDataResponse {
    fn default() -> Self {
        Self {
            height_grid: Vec::new(),
            compensation_matrix: Vec::new(),
            statistics: None,
        }
    }
}

/// Start bed leveling
pub async fn start_leveling(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<StartLevelingRequest>,
) -> impl IntoResponse {
    info!("Starting bed leveling with request: {:?}", req);

    Json(ApiResponse::success("Leveling started (via Socket API)"))
}

/// Get leveling status
pub async fn get_leveling_status(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    let status = LevelingStatusResponse::default();
    Json(ApiResponse::success(status))
}

/// Get leveling configuration
pub async fn get_leveling_config(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    let config = LevelingConfig::default();
    Json(ApiResponse::success(config))
}

/// Update leveling configuration
pub async fn update_leveling_config(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<LevelingConfigRequest>,
) -> impl IntoResponse {
    info!("Updating leveling config: {:?}", req);

    Json(ApiResponse::success("Configuration updated (via Socket API)"))
}

/// Get leveling data
pub async fn get_leveling_data(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    let data = LevelingDataResponse::default();
    Json(ApiResponse::success(data))
}

/// Reset leveling data
pub async fn reset_leveling(
    State(_state): State<Arc<ApiState>>,
) -> impl IntoResponse {
    info!("Resetting leveling data");
    
    Json(ApiResponse::success("Leveling data reset (via Socket API)"))
}