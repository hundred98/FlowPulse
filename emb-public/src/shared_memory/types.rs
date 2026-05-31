//! Shared memory types and definitions

use std::fmt;
use uuid::Uuid;

/// Shared memory error types
#[derive(Debug, Clone, PartialEq)]
pub enum SharedMemoryError {
    /// Creation failed
    CreationFailed(String),
    /// Access denied
    AccessDenied,
    /// Invalid size
    InvalidSize,
    /// Mapping failed
    MappingFailed(String),
    /// Synchronization error
    SyncError(String),
    /// Already exists
    AlreadyExists,
    /// Not found
    NotFound,
}

impl fmt::Display for SharedMemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SharedMemoryError::CreationFailed(msg) => write!(f, "Creation failed: {}", msg),
            SharedMemoryError::AccessDenied => write!(f, "Access denied"),
            SharedMemoryError::InvalidSize => write!(f, "Invalid size"),
            SharedMemoryError::MappingFailed(msg) => write!(f, "Mapping failed: {}", msg),
            SharedMemoryError::SyncError(msg) => write!(f, "Sync error: {}", msg),
            SharedMemoryError::AlreadyExists => write!(f, "Already exists"),
            SharedMemoryError::NotFound => write!(f, "Not found"),
        }
    }
}

impl std::error::Error for SharedMemoryError {}

/// Shared memory configuration
#[derive(Debug, Clone)]
pub struct SharedMemoryConfig {
    /// Unique identifier for the shared memory segment
    pub name: String,
    /// Size in bytes
    pub size: usize,
    /// Access permissions
    pub permissions: SharedMemoryPermissions,
    /// Memory alignment (bytes)
    pub alignment: usize,
    /// Enable cache optimization
    pub cache_optimized: bool,
}

impl Default for SharedMemoryConfig {
    fn default() -> Self {
        Self {
            name: "default_shm".to_string(),
            size: 1024 * 1024,
            permissions: SharedMemoryPermissions::ReadWrite,
            alignment: 64,
            cache_optimized: true,
        }
    }
}

impl SharedMemoryConfig {
    /// Create new configuration
    pub fn new(name: &str, size: usize) -> Self {
        Self {
            name: name.to_string(),
            size,
            permissions: SharedMemoryPermissions::ReadWrite,
            alignment: 64,
            cache_optimized: true,
        }
    }

    /// Set permissions
    pub fn with_permissions(mut self, permissions: SharedMemoryPermissions) -> Self {
        self.permissions = permissions;
        self
    }

    /// Set alignment
    pub fn with_alignment(mut self, alignment: usize) -> Self {
        self.alignment = alignment;
        self
    }

    /// Enable/disable cache optimization
    pub fn with_cache_optimization(mut self, enabled: bool) -> Self {
        self.cache_optimized = enabled;
        self
    }
}

/// Shared memory permissions
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SharedMemoryPermissions {
    /// Read only
    ReadOnly,
    /// Read and write
    ReadWrite,
    /// Execute, read, and write
    ExecuteReadWrite,
}

/// Shared memory statistics
#[derive(Debug, Clone)]
pub struct SharedMemoryStats {
    /// Process ID
    pub process_id: u32,
    /// Total number of segments
    pub total_segments: usize,
    /// Total memory usage in bytes
    pub total_memory_bytes: usize,
    /// Manager uptime in seconds
    pub uptime: u64,
}

/// Memory region
#[derive(Debug)]
pub struct MemoryRegion<T> {
    /// Unique identifier
    pub id: String,
    /// Memory region data
    pub data: Vec<T>,
    /// Size in bytes
    pub data_size: usize,
    /// Read-only flag
    pub read_only: bool,
}

unsafe impl<T> Send for MemoryRegion<T> {}
unsafe impl<T> Sync for MemoryRegion<T> {}

impl<T> MemoryRegion<T> {
    /// Create new memory region
    pub fn new(id: String, data: Vec<T>) -> Self {
        Self {
            id,
            data_size: data.len() * std::mem::size_of::<T>(),
            read_only: false,
            data,
        }
    }

    /// Get raw pointer
    pub fn as_ptr(&self) -> *mut T {
        self.data.as_ptr() as *mut T
    }

    /// Get slice reference
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    /// Get mutable slice reference
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }

    /// Get element at index
    pub fn get(&self, index: usize) -> &T {
        &self.data[index]
    }

    /// Get mutable element at index
    pub fn get_mut(&mut self, index: usize) -> &mut T {
        &mut self.data[index]
    }

    /// Get region size
    pub fn size(&self) -> usize {
        self.data_size
    }

    /// Get region ID
    pub fn region_id(&self) -> Uuid {
        Uuid::parse_str(&self.id).unwrap_or_default()
    }
}

impl<T> Drop for MemoryRegion<T> {
    fn drop(&mut self) {
    }
}

// === Server Types ===

pub const SHM_NAME: &str = "3d_printer_shm";
pub const SHM_SIZE: usize = 32768;
pub const SHM_MAGIC: u32 = 0xDEADBEEF;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PrinterCmd {
    None = 0,
    StartPrint = 1,
    PausePrint = 2,
    ResumePrint = 3,
    StopPrint = 4,
    SetHotendTemp = 5,
    SetBedTemp = 6,
    Home = 7,
    EmergencyStop = 8,
}

impl Default for PrinterCmd {
    fn default() -> Self {
        PrinterCmd::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PrinterState {
    Idle = 0,
    Starting = 1,
    Printing = 2,
    Paused = 3,
    Resuming = 4,
    Stopping = 5,
    Completed = 6,
    Failed = 7,
}

impl Default for PrinterState {
    fn default() -> Self {
        PrinterState::Idle
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PrinterStatus {
    pub magic: u32,
    pub version: u32,
    pub timestamp: u32,
    pub state: PrinterState,
    pub hotend_temp: f32,
    pub hotend_target: f32,
    pub bed_temp: f32,
    pub bed_target: f32,
    pub position_x: f32,
    pub position_y: f32,
    pub position_z: f32,
    pub progress: f32,
    pub current_layer: i32,
    pub total_layers: i32,
    pub remaining_time_sec: i32,
    pub flags: u32,
}

impl Default for PrinterStatus {
    fn default() -> Self {
        Self {
            magic: SHM_MAGIC,
            version: 1,
            timestamp: 0,
            state: PrinterState::Idle,
            hotend_temp: 0.0,
            hotend_target: 0.0,
            bed_temp: 0.0,
            bed_target: 0.0,
            position_x: 0.0,
            position_y: 0.0,
            position_z: 0.0,
            progress: 0.0,
            current_layer: 0,
            total_layers: 0,
            remaining_time_sec: 0,
            flags: 0,
        }
    }
}

impl PrinterStatus {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PrinterCommand {
    pub cmd: PrinterCmd,
    pub param1: u32,
    pub param2: u32,
    pub param_float: f32,
    pub filename: [u8; 256],
    pub timestamp: u32,
}

impl Default for PrinterCommand {
    fn default() -> Self {
        Self {
            cmd: PrinterCmd::None,
            param1: 0,
            param2: 0,
            param_float: 0.0,
            filename: [0u8; 256],
            timestamp: 0,
        }
    }
}

impl PrinterCommand {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn set_filename(&mut self, name: &str) {
        let bytes = name.as_bytes();
        let len = bytes.len().min(255);
        self.filename[..len].copy_from_slice(&bytes[..len]);
        if len < 256 {
            self.filename[len] = 0;
        }
    }
    
    pub fn get_filename(&self) -> String {
        let len = self.filename.iter().position(|&x| x == 0).unwrap_or(256);
        String::from_utf8_lossy(&self.filename[..len]).to_string()
    }
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct CommandQueue {
    pub head: u32,
    pub tail: u32,
    pub count: u32,
    pub commands: [PrinterCommand; 32],
}

impl Default for CommandQueue {
    fn default() -> Self {
        Self {
            head: 0,
            tail: 0,
            count: 0,
            commands: [PrinterCommand::new(); 32],
        }
    }
}

impl CommandQueue {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct SharedMemory {
    pub status: PrinterStatus,
    pub cmd_queue: CommandQueue,
    pub ack_queue: CommandQueue,
}

impl SharedMemory {
    pub fn new() -> Self {
        Self::default()
    }
}