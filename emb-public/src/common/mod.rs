//! Common utilities module
//!
//! Shared utilities used across multiple modules.

pub mod error;
pub mod pin_parser;

pub use error::{EmbError, EmbResult};
pub use pin_parser::parse_pin;
