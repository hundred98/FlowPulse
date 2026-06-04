//! Message queue system for 3D printer firmware
//!
//! This module provides message queue capabilities for inter-component communication.

pub mod types;
pub mod handler;
pub mod queue;

pub use types::{
    Message, MessageType, MessagePriority, MessageStatus,
    MessageQueueConfig, QueueStats,
};
pub use handler::MessageHandler;
pub use queue::MessageQueue;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EventListener;
    use std::sync::Arc;
    
    struct TestHandler {
        name: String,
    }
    
    impl TestHandler {
        fn new(name: String) -> Self {
            Self { name }
        }
    }
    
    #[async_trait::async_trait]
    impl MessageHandler for TestHandler {
        async fn handle(&self, message: &mut Message) -> crate::EmbResult<()> {
            message.mark_completed();
            Ok(())
        }
        
        fn name(&self) -> &str {
            &self.name
        }
    }
    
    #[tokio::test]
    async fn test_message_creation() {
        let message = Message::new(
            MessageType::SystemCommand,
            "test".to_string(),
            serde_json::json!({"command": "test"}),
        ).with_priority(MessagePriority::High);
        
        assert_eq!(message.message_type, MessageType::SystemCommand);
        assert_eq!(message.priority, MessagePriority::High);
        assert_eq!(message.status, MessageStatus::Pending);
    }
    
    #[tokio::test]
    async fn test_message_queue() -> crate::EmbResult<()> {
        let queue = MessageQueue::default();
        let handler = Arc::new(TestHandler::new("test_handler".to_string()));
        
        queue.add_handler(MessageType::SystemCommand, handler).await;
        
        let message = Message::new(
            MessageType::SystemCommand,
            "test".to_string(),
            serde_json::json!({"command": "test"}),
        );
        
        queue.enqueue(message).await?;
        
        let stats = queue.get_stats().await;
        assert_eq!(stats.pending_count, 1);
        
        Ok(())
    }
}