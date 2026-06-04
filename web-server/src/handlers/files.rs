//! File Management Handlers
//!
//! HTTP handlers for file management endpoints.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::WebServerState;

/// File information
#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub modified: String,
}

/// List files
pub async fn list_files(
    State(_state): State<Arc<WebServerState>>,
) -> Result<Json<Vec<FileInfo>>, StatusCode> {
    // TODO: Implement actual file listing
    let files = vec![
        FileInfo {
            name: "test.gcode".to_string(),
            size: 1024,
            modified: "2024-01-01T00:00:00Z".to_string(),
        },
    ];
    
    Ok(Json(files))
}
