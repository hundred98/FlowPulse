//! Message types and definitions

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message priority levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl MessagePriority {
    /// Get the ordinal value of the priority
    pub fn ordinal(&self) -> u8 {
        match self {
            MessagePriority::Low => 0,
            MessagePriority::Normal => 1,
            MessagePriority::High => 2,
            MessagePriority::Critical => 3,
        }
    }
}

impl Default for MessagePriority {
    fn default() -> Self {
        MessagePriority::Normal
    }
}

/// Message types for the 3D printer system
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageType {
    // System messages
    SystemCommand,
    SystemResponse,
    SystemEvent,
    
    // State machine messages
    StateTransition,
    StateQuery,
    StateResponse,
    
    // Print job messages
    PrintStart,
    PrintPause,
    PrintResume,
    PrintStop,
    PrintComplete,
    PrintError,
    
    // Motion control messages
    MoveCommand,
    MoveResponse,
    HomeCommand,
    HomeResponse,
    
    // Temperature control messages
    TemperatureSet,
    TemperatureGet,
    TemperatureUpdate,
    
    // G-code messages
    GcodeLine,
    GcodeResponse,
    
    // Hardware messages
    HardwareStatus,
    HardwareError,
    HardwareCommand,
    
    // Communication messages
    NetworkMessage,
    SerialMessage,
    
    // Custom messages
    Custom(String),
}

/// Message status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Timeout,
    Cancelled,
}

/// A message in the queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier
    pub id: Uuid,
    
    /// Message type
    pub message_type: MessageType,
    
    /// Message priority
    pub priority: MessagePriority,
    
    /// Message payload
    pub payload: serde_json::Value,
    
    /// Message source
    pub source: String,
    
    /// Message destination (optional)
    pub destination: Option<String>,
    
    /// Message status
    pub status: MessageStatus,
    
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Processing start timestamp
    pub started_at: Option<DateTime<Utc>>,
    
    /// Completion timestamp
    pub completed_at: Option<DateTime<Utc>>,
    
    /// Timeout in milliseconds
    pub timeout_ms: Option<u64>,
    
    /// Retry count
    pub retry_count: u32,
    
    /// Maximum retries allowed
    pub max_retries: u32,
    
    /// Correlation ID for request-response pairs
    pub correlation_id: Option<Uuid>,
}

impl Message {
    /// Create a new message
    pub fn new(message_type: MessageType, source: String, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            message_type,
            priority: MessagePriority::default(),
            payload,
            source,
            destination: None,
            status: MessageStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            timeout_ms: None,
            retry_count: 0,
            max_retries: 3,
            correlation_id: None,
        }
    }
    
    /// Set message priority
    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }
    
    /// Set message destination
    pub fn with_destination(mut self, destination: String) -> Self {
        self.destination = Some(destination);
        self
    }
    
    /// Set timeout
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
    
    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }
    
    /// Set max retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }
    
    /// Mark message as processing
    pub fn mark_processing(&mut self) {
        self.status = MessageStatus::Processing;
        self.started_at = Some(Utc::now());
    }
    
    /// Mark message as completed
    pub fn mark_completed(&mut self) {
        self.status = MessageStatus::Completed;
        self.completed_at = Some(Utc::now());
    }
    
    /// Mark message as failed
    pub fn mark_failed(&mut self) {
        self.status = MessageStatus::Failed;
        self.completed_at = Some(Utc::now());
    }
    
    /// Mark message as timed out
    pub fn mark_timeout(&mut self) {
        self.status = MessageStatus::Timeout;
        self.completed_at = Some(Utc::now());
    }
    
    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
    
    /// Check if message can be retried
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries && 
        matches!(self.status, MessageStatus::Failed | MessageStatus::Timeout)
    }
    
    /// Check if message is timed out
    pub fn is_timed_out(&self) -> bool {
        if let (Some(timeout_ms), Some(started_at)) = (self.timeout_ms, self.started_at) {
            let elapsed = Utc::now().signed_duration_since(started_at);
            elapsed.num_milliseconds() > timeout_ms as i64
        } else {
            false
        }
    }
    
    /// Get processing duration
    pub fn processing_duration(&self) -> Option<chrono::Duration> {
        if let (Some(started_at), Some(completed_at)) = (self.started_at, self.completed_at) {
            Some(completed_at.signed_duration_since(started_at))
        } else {
            None
        }
    }
}

/// Message queue configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageQueueConfig {
    /// Maximum queue size
    pub max_queue_size: usize,
    
    /// Maximum concurrent processing
    pub max_concurrent: usize,
    
    /// Default timeout in milliseconds
    pub default_timeout_ms: u64,
    
    /// Enable message persistence
    pub enable_persistence: bool,
    
    /// Batch processing size
    pub batch_size: usize,
    
    /// Queue check interval in milliseconds
    pub check_interval_ms: u64,
}

impl Default for MessageQueueConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 10000,
            max_concurrent: 10,
            default_timeout_ms: 5000,
            enable_persistence: false,
            batch_size: 10,
            check_interval_ms: 100,
        }
    }
}

/// Message queue statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    /// Total messages processed
    pub total_processed: u64,
    
    /// Messages currently in queue
    pub pending_count: usize,
    
    /// Messages currently processing
    pub processing_count: usize,
    
    /// Messages completed successfully
    pub completed_count: u64,
    
    /// Messages failed
    pub failed_count: u64,
    
    /// Messages timed out
    pub timeout_count: u64,
    
    /// Average processing time in milliseconds
    pub avg_processing_time_ms: f64,
    
    /// Queue utilization (0.0 to 1.0)
    pub utilization: f64,
}