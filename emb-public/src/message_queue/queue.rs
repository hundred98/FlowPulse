//! Priority message queue implementation

use crate::{EmbError, EmbResult, PrinterEvent, EventKind, SyncEventPublisher};
use chrono::Utc;
use crossbeam::queue::SegQueue;
use serde_json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{RwLock, Notify};
use uuid::Uuid;

use super::types::{Message, MessageType, MessageStatus, MessageQueueConfig, QueueStats};
use super::handler::MessageHandler;

/// Priority message queue
pub struct MessageQueue {
    /// Priority queues (one for each priority level)
    queues: Vec<Arc<SegQueue<Message>>>,
    
    /// Message handlers by message type
    handlers: Arc<RwLock<std::collections::HashMap<MessageType, Arc<dyn MessageHandler>>>>,
    
    /// Event publisher
    event_publisher: Arc<Mutex<SyncEventPublisher>>,
    
    /// Configuration
    config: MessageQueueConfig,
    
    /// Statistics
    stats: Arc<RwLock<QueueStats>>,
    
    /// Shutdown notification
    shutdown_notify: Arc<Notify>,
    
    /// Queue ID
    id: Uuid,
}

impl MessageQueue {
    /// Create a new message queue
    pub fn new(config: MessageQueueConfig) -> Self {
        let queues: Vec<Arc<SegQueue<Message>>> = (0..4)
            .map(|_| Arc::new(SegQueue::new()))
            .collect();
        
        Self {
            queues,
            handlers: Arc::new(RwLock::new(std::collections::HashMap::new())),
            event_publisher: Arc::new(Mutex::new(SyncEventPublisher::new())),
            config,
            stats: Arc::new(RwLock::new(QueueStats {
                total_processed: 0,
                pending_count: 0,
                processing_count: 0,
                completed_count: 0,
                failed_count: 0,
                timeout_count: 0,
                avg_processing_time_ms: 0.0,
                utilization: 0.0,
            })),
            shutdown_notify: Arc::new(Notify::new()),
            id: Uuid::new_v4(),
        }
    }
    
    /// Add a message handler
    pub async fn add_handler(&self, message_type: MessageType, handler: Arc<dyn MessageHandler>) {
        let mut handlers = self.handlers.write().await;
        handlers.insert(message_type, handler);
    }
    
    /// Enqueue a message
    pub async fn enqueue(&self, mut message: Message) -> EmbResult<()> {
        // Check queue size limit
        let pending_count = self.get_pending_count();
        if pending_count >= self.config.max_queue_size {
            return Err(EmbError::MessageQueue("Queue is full".to_string()));
        }
        
        // Set default timeout if not specified
        if message.timeout_ms.is_none() {
            message.timeout_ms = Some(self.config.default_timeout_ms);
        }
        
        // Add to appropriate priority queue
        let priority_index = message.priority.ordinal() as usize;
        if priority_index < self.queues.len() {
            self.queues[priority_index].push(message.clone());
            
            // Publish message enqueued event
            let event = PrinterEvent::new(
                EventKind::MessageReceived,
                "message_queue".to_string(),
                format!("Message {} enqueued with priority {:?}", message.id, message.priority),
            ).with_data(serde_json::json!({
                "message_id": message.id,
                "message_type": message.message_type,
                "priority": message.priority,
                "source": message.source,
            }));
            
            self.event_publisher.lock().unwrap().publish_sync(event);
            
            log::debug!("Message {} enqueued: {:?}", message.id, message.message_type);
            Ok(())
        } else {
            Err(EmbError::MessageQueue("Invalid priority".to_string()))
        }
    }
    
    /// Dequeue a message (highest priority first)
    pub fn dequeue(&self) -> Option<Message> {
        // Check queues in priority order (highest to lowest)
        for queue in self.queues.iter().rev() {
            if let Some(message) = queue.pop() {
                return Some(message);
            }
        }
        None
    }
    
    /// Get pending message count
    pub fn get_pending_count(&self) -> usize {
        self.queues.iter().map(|q| q.len()).sum()
    }
    
    /// Get queue statistics
    pub async fn get_stats(&self) -> QueueStats {
        let stats = self.stats.read().await.clone();
        let pending_count = self.get_pending_count();
        
        QueueStats {
            pending_count,
            utilization: pending_count as f64 / self.config.max_queue_size as f64,
            ..stats
        }
    }
    
    /// Process messages continuously
    pub async fn start_processing(&self) -> EmbResult<()> {
        log::info!("Starting message queue processing");
        
        let shutdown = self.shutdown_notify.notified();
        tokio::pin!(shutdown);
        
        loop {
            // Process a batch of messages
            match self.process_batch().await {
                Ok(_) => {
                    // Continue processing
                }
                Err(e) => {
                    log::error!("Error in message processing: {}", e);
                }
            }
            
            // Check for shutdown or wait
            tokio::select! {
                _ = &mut shutdown => {
                    log::info!("Message queue processing stopped");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(self.config.check_interval_ms)) => {
                    // Continue processing
                }
            }
        }
        
        Ok(())
    }
    
    /// Process a batch of messages
    async fn process_batch(&self) -> EmbResult<()> {
        let mut processed = 0;
        
        while processed < self.config.batch_size {
            if let Some(mut message) = self.dequeue() {
                // Check if message is timed out
                if message.is_timed_out() {
                    message.mark_timeout();
                    self.handle_timeout(&message).await;
                    processed += 1;
                    continue;
                }
                
                // Get handler for this message type
                let handlers = self.handlers.read().await;
                if let Some(handler) = handlers.get(&message.message_type) {
                    // Mark as processing
                    message.mark_processing();
                    
                    // Handle message
                    let start_time = Utc::now();
                    let result = handler.handle(&mut message).await;
                    let processing_time = Utc::now().signed_duration_since(start_time);
                    
                    // Update statistics
                    self.update_stats(&message, &result, processing_time).await;
                    
                    match result {
                        Ok(_) => {
                            message.mark_completed();
                            log::debug!("Message {} processed successfully", message.id);
                        }
                        Err(e) => {
                            message.mark_failed();
                            log::error!("Message {} processing failed: {}", message.id, e);
                            
                            // Retry if possible
                            if message.can_retry() {
                                let mut retry_message = message.clone();
                                retry_message.increment_retry();
                                retry_message.status = MessageStatus::Pending;
                                self.enqueue(retry_message).await?;
                            }
                        }
                    }
                    
                    // Publish message processed event
                    let event = PrinterEvent::new(
                        EventKind::MessageSent,
                        "message_queue".to_string(),
                        format!("Message {} processed: {:?}", message.id, message.status),
                    ).with_data(serde_json::json!({
                        "message_id": message.id,
                        "status": message.status,
                        "processing_time_ms": processing_time.num_milliseconds(),
                        "handler": handler.name(),
                    }));
                    
                    self.event_publisher.lock().unwrap().publish_sync(event);
                } else {
                    // No handler found
                    message.mark_failed();
                    log::warn!("No handler found for message type: {:?}", message.message_type);
                }
                
                processed += 1;
            } else {
                // No more messages, break to avoid blocking
                break;
            }
        }
        
        Ok(())
    }
    
    /// Handle timed out message
    async fn handle_timeout(&self, message: &Message) {
        let mut stats = self.stats.write().await;
        stats.timeout_count += 1;
        
        // Publish timeout event
        let event = PrinterEvent::warning(
            "message_queue".to_string(),
            format!("Message {} timed out", message.id),
        ).with_data(serde_json::json!({
            "message_id": message.id,
            "timeout_ms": message.timeout_ms,
        }));
        
        self.event_publisher.lock().unwrap().publish_sync(event);
    }
    
    /// Update queue statistics
    async fn update_stats(&self, message: &Message, result: &EmbResult<()>, _processing_time: chrono::Duration) {
        let mut stats = self.stats.write().await;
        stats.total_processed += 1;
        
        match result {
            Ok(_) => stats.completed_count += 1,
            Err(_) => stats.failed_count += 1,
        }
        
        // Update average processing time
        if let Some(duration) = message.processing_duration() {
            let total_time = stats.avg_processing_time_ms * (stats.total_processed - 1) as f64;
            stats.avg_processing_time_ms = (total_time + duration.num_milliseconds() as f64) / stats.total_processed as f64;
        }
    }
    
    /// Shutdown the message queue
    pub async fn shutdown(&self) {
        self.shutdown_notify.notify_waiters();
    }
    
    /// Get queue ID
    pub fn id(&self) -> Uuid {
        self.id
    }
    
    /// Get configuration
    pub fn config(&self) -> &MessageQueueConfig {
        &self.config
    }
}

impl Default for MessageQueue {
    fn default() -> Self {
        Self::new(MessageQueueConfig::default())
    }
}