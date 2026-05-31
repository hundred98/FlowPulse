//! Shared memory module for extreme low-latency data sharing
//!
//! This module provides high-performance shared memory primitives for 3D printer
//! real-time control loops, enabling microsecond-level latency communication
//! between processes.
//!
//! Note: On Windows, this uses a Vec<u8>-based fallback instead of POSIX shared memory.

pub mod types;
pub mod manager;
pub mod server;

pub use types::{
    SharedMemoryError, SharedMemoryConfig, SharedMemoryPermissions,
    SharedMemoryStats, MemoryRegion,
    SHM_NAME, SHM_SIZE, SHM_MAGIC,
    PrinterCmd, PrinterState, PrinterStatus, PrinterCommand,
    CommandQueue, SharedMemory,
};
pub use manager::{SharedMemoryManager, SharedMemoryHandle, SharedMemorySegment};
pub use server::SharedMemoryServer;