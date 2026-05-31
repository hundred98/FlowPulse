//! Application logic
//!
//! Contains the main application state and logic.
//! Uses CoreSocketClient for communication with emb-core-server.

use std::sync::Arc;
use emb_public::CoreSocketClient;

pub struct AppState {
    #[allow(dead_code)]
    pub core_client: Arc<CoreSocketClient>,
}

impl AppState {
    #[allow(dead_code)]
    pub fn new(core_client: Arc<CoreSocketClient>) -> Self {
        Self { core_client }
    }
}