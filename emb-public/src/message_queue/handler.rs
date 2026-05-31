//! Message handler trait

use crate::EmbResult;
use super::types::Message;

/// Message handler trait
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle a message
    async fn handle(&self, message: &mut Message) -> EmbResult<()>;
    
    /// Get handler name
    fn name(&self) -> &str;
}