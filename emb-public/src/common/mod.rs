//! Common utilities module
//!
//! Shared utilities used across multiple modules.

pub mod error;
pub mod events;
pub mod debug;
pub mod geometry;
pub mod pin_parser;

pub use error::{EmbError, EmbResult};
pub use events::{PrinterEvent, EventKind, EventSeverity, EventListener, EventPublisher, SyncEventPublisher};
pub use debug::{init_debug, is_debug_enabled};
pub use geometry::ArcGeometry;
pub use pin_parser::parse_pin;
