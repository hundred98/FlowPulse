//! Common utilities module
//!
//! Shared utilities used across multiple modules.

pub mod error;
pub mod events;
pub mod messages;
pub mod pin_parser;

pub use error::{EmbError, EmbResult};
pub use events::{
    PrinterEvent, EventKind, EventSeverity, EventListener,
    EventPublisher, SyncEventPublisher,
};
pub use messages::{
    WebSocketMessage, SharedState, PrinterStatus, TempStatus, PositionData,
};
pub use pin_parser::parse_pin;
