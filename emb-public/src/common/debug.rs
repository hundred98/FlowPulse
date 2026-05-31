//! Global debug control module
//!
//! Controlled by `RUST_DEBUG` environment variable.
//! - `RUST_DEBUG=1` or `RUST_DEBUG=true`: Enable all debug output
//! - `RUST_DEBUG=0` or `RUST_DEBUG=false`: Disable all debug output
//! - Default: Enable debug output

use std::sync::OnceLock;

static DEBUG_ENABLED: OnceLock<bool> = OnceLock::new();

pub fn init_debug(enabled: bool) {
    DEBUG_ENABLED.set(enabled).ok();
}

pub fn is_debug_enabled() -> bool {
    *DEBUG_ENABLED.get_or_init(|| {
        std::env::var("RUST_DEBUG")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(true)
    })
}

#[macro_export]
macro_rules! debug_print {
    ($($arg:tt)*) => {
        if $crate::common::debug::is_debug_enabled() {
            print!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! debug_println {
    () => {
        if $crate::common::debug::is_debug_enabled() {
            println!();
        }
    };
    ($($arg:tt)*) => {
        if $crate::common::debug::is_debug_enabled() {
            println!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if $crate::common::debug::is_debug_enabled() {
            log::info!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! vprintln {
    () => {
        if $crate::common::debug::is_debug_enabled() {
            println!();
        }
    };
    ($($arg:tt)*) => {
        if $crate::common::debug::is_debug_enabled() {
            println!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! veprintln {
    ($($arg:tt)*) => {
        if $crate::common::debug::is_debug_enabled() {
            eprintln!($($arg)*);
        }
    };
}

pub fn init() {
    let enabled = std::env::var("RUST_DEBUG")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(true);
    init_debug(enabled);
}
