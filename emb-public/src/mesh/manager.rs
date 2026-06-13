//! Mesh Manager
//!
//! Provides operations for bed mesh data management,
//! including clearing, querying, and sending mesh data.

use std::sync::Arc;
use crate::CoreSocketClient;
use emb_api::{CoreRequest, MotionRequest, CoreResponse, MotionResponse};

/// Mesh Manager for bed mesh operations.
///
/// Wraps the core socket client to provide high-level mesh operations
/// such as clearing mesh data on the server.
pub struct MeshManager {
    client: Arc<CoreSocketClient>,
}

impl MeshManager {
    /// Create a new MeshManager with the given core socket client.
    pub fn new(client: Arc<CoreSocketClient>) -> Self {
        Self { client }
    }

    /// Get a reference to the underlying core socket client.
    pub fn client(&self) -> &Arc<CoreSocketClient> {
        &self.client
    }

    /// Clear mesh data on the server.
    ///
    /// Sends a `ClearMesh` request to reset the mesh compensation state
    /// and return to the Empty state.
    ///
    /// # Returns
    /// * `Ok(())` - If mesh data was cleared successfully
    /// * `Err(String)` - If the request failed
    pub async fn clear_mesh(&self) -> Result<(), String> {
        log::info!("Clearing mesh data on server...");

        let request = CoreRequest::Motion(MotionRequest::ClearMesh);
        match self.client.send_request(&request).await {
            Ok(CoreResponse::Motion(MotionResponse::MeshComplete)) => {
                log::info!("✅ Mesh data cleared successfully");
                Ok(())
            }
            Ok(CoreResponse::Motion(MotionResponse::MeshNack { reason, .. })) => {
                let msg = format!("Mesh clear failed (NACK: {:?})", reason);
                log::error!("❌ {}", msg);
                Err(msg)
            }
            Ok(CoreResponse::Error(e)) => {
                let msg = format!("Mesh clear failed: {}", e.message);
                log::error!("❌ {}", msg);
                Err(msg)
            }
            Ok(other) => {
                log::info!("✅ Mesh cleared (response: {:?})", other);
                Ok(())
            }
            Err(e) => {
                let msg = format!("Mesh clear request failed: {}", e);
                log::error!("❌ {}", msg);
                Err(msg)
            }
        }
    }
}