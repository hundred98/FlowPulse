//! Error types for the 3D printer firmware system

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmbError {
    #[error("State machine error: {0}")]
    StateMachine(String),
    
    #[error("Invalid state transition: from {from:?} to {to:?}")]
    InvalidTransition { from: String, to: String },
    
    #[error("Message queue error: {0}")]
    MessageQueue(String),
    
    #[error("Communication error: {0}")]
    Communication(String),
    
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Timeout error: operation timed out after {0}ms")]
    Timeout(u64),
    
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    #[error("Config error: {0}")]
    Config(String),
    
    #[error("Motion control error: {0}")]
    MotionControl(String),
    
    #[error("G-code parse error: {0}")]
    GCodeParse(String),
    
    #[error("File not found: {0}")]
    FileNotFound(String),
    
    #[error("Hardware error: {0}")]
    Hardware(String),
    
    #[error("Safety error: {0}")]
    Safety(String),
    
    #[error("Serial frame lost: seq={seq}, retries={retries}")]
    FrameLost { seq: u8, retries: u32 },
    
    #[error("Serial communication fatal error: {0}")]
    SerialFatal(String),
    
    #[error("Crypto error: {0}")]
    CryptoError(String),
}

pub type EmbResult<T> = Result<T, EmbError>;
