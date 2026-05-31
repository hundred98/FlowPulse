//! Shared memory manager implementation

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[cfg(unix)]
use libc;

use super::types::{
    SharedMemoryError, SharedMemoryConfig,
    SharedMemoryStats, MemoryRegion,
};

/// Shared memory segment
pub struct SharedMemorySegment {
    /// Segment configuration
    config: SharedMemoryConfig,
    /// Memory mapping (Unix) or buffer (Windows)
    #[cfg(unix)]
    memory_ptr: *mut u8,
    /// Buffer for Windows fallback
    #[cfg(windows)]
    buffer: Vec<u8>,
    /// Size in bytes
    size: usize,
    /// File descriptor (Unix only)
    #[cfg(unix)]
    fd: i32,
    /// Reference count
    ref_count: Arc<Mutex<u32>>,
}

unsafe impl Send for SharedMemorySegment {}
unsafe impl Sync for SharedMemorySegment {}

impl SharedMemorySegment {
    /// Create new shared memory segment
    #[cfg(unix)]
    fn create(config: &SharedMemoryConfig) -> Result<Self, SharedMemoryError> {
        if config.size == 0 {
            return Err(SharedMemoryError::InvalidSize);
        }

        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        let aligned_size = ((config.size + page_size - 1) / page_size) * page_size;

        let name_cstr = std::ffi::CString::new(config.name.as_str())
            .map_err(|e| SharedMemoryError::CreationFailed(e.to_string()))?;
        
        let fd = unsafe {
            libc::shm_open(
                name_cstr.as_ptr(),
                libc::O_CREAT | libc::O_RDWR,
                0o666,
            )
        };

        if fd == -1 {
            return Err(SharedMemoryError::CreationFailed(
                "Failed to create shared memory".to_string()
            ));
        }

        let result = unsafe { libc::ftruncate(fd, aligned_size as libc::off_t) };
        if result == -1 {
            unsafe { libc::close(fd) };
            return Err(SharedMemoryError::CreationFailed(
                "Failed to set shared memory size".to_string()
            ));
        }

        let memory_ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                aligned_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };

        if memory_ptr == libc::MAP_FAILED {
            unsafe { libc::close(fd) };
            return Err(SharedMemoryError::MappingFailed(
                "Failed to map shared memory".to_string()
            ));
        }

        Ok(Self {
            config: config.clone(),
            memory_ptr: memory_ptr as *mut u8,
            size: aligned_size,
            fd,
            ref_count: Arc::new(Mutex::new(1)),
        })
    }

    /// Create new shared memory segment (Windows fallback)
    #[cfg(windows)]
    fn create(config: &SharedMemoryConfig) -> Result<Self, SharedMemoryError> {
        if config.size == 0 {
            return Err(SharedMemoryError::InvalidSize);
        }

        let aligned_size = config.size;
        let buffer = vec![0u8; aligned_size];

        Ok(Self {
            config: config.clone(),
            buffer,
            size: aligned_size,
            ref_count: Arc::new(Mutex::new(1)),
        })
    }

    /// Get memory pointer
    #[cfg(unix)]
    fn get_memory_ptr(&self) -> *mut u8 {
        self.memory_ptr
    }

    /// Get memory pointer (Windows fallback)
    #[cfg(windows)]
    fn get_memory_ptr(&self) -> *mut u8 {
        self.buffer.as_ptr() as *mut u8
    }

    /// Get segment ID
    fn get_id(&self) -> Uuid {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        self.config.name.hash(&mut hasher);
        let hash = hasher.finish();
        
        Uuid::from_u64_pair(hash, 0)
    }

    /// Get configuration
    fn get_config(&self) -> SharedMemoryConfig {
        self.config.clone()
    }

    /// Get size
    fn get_size(&self) -> usize {
        self.size
    }
}

#[cfg(unix)]
impl Drop for SharedMemorySegment {
    fn drop(&mut self) {
        if !self.memory_ptr.is_null() {
            unsafe {
                libc::munmap(self.memory_ptr as *mut libc::c_void, self.size);
            }
        }

        if self.fd != -1 {
            unsafe {
                libc::close(self.fd);
            }
        }

        let name_cstr = std::ffi::CString::new(self.config.name.as_str()).unwrap();
        unsafe {
            libc::shm_unlink(name_cstr.as_ptr());
        }
    }
}

#[cfg(windows)]
impl Drop for SharedMemorySegment {
    fn drop(&mut self) {
    }
}

/// High-performance shared memory manager
pub struct SharedMemoryManager {
    /// Shared memory segments
    segments: Arc<Mutex<HashMap<String, SharedMemorySegment>>>,
    /// Process ID
    process_id: u32,
}

impl SharedMemoryManager {
    /// Create new shared memory manager
    pub fn new() -> Result<Self, SharedMemoryError> {
        Ok(Self {
            segments: Arc::new(Mutex::new(HashMap::new())),
            process_id: std::process::id(),
        })
    }

    /// Create or open shared memory segment
    pub fn create_segment(
        &self,
        config: SharedMemoryConfig,
    ) -> Result<SharedMemoryHandle, SharedMemoryError> {
        let mut segments = self.segments.lock().unwrap();
        
        if segments.contains_key(&config.name) {
            return Err(SharedMemoryError::AlreadyExists);
        }

        let segment = SharedMemorySegment::create(&config)?;
        let segment_id = segment.get_id();
        
        segments.insert(config.name.clone(), segment);
        
        Ok(SharedMemoryHandle {
            segment_id,
            config,
            segments: Arc::clone(&self.segments),
        })
    }

    /// Open existing shared memory segment
    pub fn open_segment(
        &self,
        name: &str,
    ) -> Result<SharedMemoryHandle, SharedMemoryError> {
        let segments = self.segments.lock().unwrap();
        
        if let Some(segment) = segments.get(name) {
            Ok(SharedMemoryHandle {
                segment_id: segment.get_id(),
                config: segment.get_config(),
                segments: Arc::clone(&self.segments),
            })
        } else {
            Err(SharedMemoryError::NotFound)
        }
    }

    /// Remove shared memory segment
    pub fn remove_segment(&self, name: &str) -> Result<(), SharedMemoryError> {
        let mut segments = self.segments.lock().unwrap();
        
        if let Some(segment) = segments.remove(name) {
            drop(segment);
            Ok(())
        } else {
            Err(SharedMemoryError::NotFound)
        }
    }

    /// List all segments
    pub fn list_segments(&self) -> Vec<String> {
        self.segments.lock().unwrap().keys().cloned().collect()
    }

    /// Get statistics
    pub fn get_stats(&self) -> SharedMemoryStats {
        let segments = self.segments.lock().unwrap();
        let total_segments = segments.len();
        let total_memory: usize = segments.values()
            .map(|s| s.get_size())
            .sum();

        SharedMemoryStats {
            process_id: self.process_id,
            total_segments,
            total_memory_bytes: total_memory,
            uptime: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

impl Default for SharedMemoryManager {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

/// Handle for accessing shared memory segment
#[derive(Clone)]
pub struct SharedMemoryHandle {
    segment_id: Uuid,
    config: SharedMemoryConfig,
    segments: Arc<Mutex<HashMap<String, SharedMemorySegment>>>,
}

impl SharedMemoryHandle {
    /// Create typed memory region
    pub fn create_region<T: Clone>(&self, offset: usize, count: usize) -> Result<MemoryRegion<T>, SharedMemoryError> {
        let segments = self.segments.lock().unwrap();
        
        for segment in segments.values() {
            if segment.get_id() == self.segment_id {
                let base_ptr = unsafe {
                    segment.get_memory_ptr()
                        .add(offset)
                };
                
                let data = unsafe {
                    std::slice::from_raw_parts(base_ptr as *const T, count).to_vec()
                };
                
                let id = Uuid::new_v4().to_string();
                let region = MemoryRegion::new(id.clone(), data);
                return Ok(region);
            }
        }
        
        Err(SharedMemoryError::NotFound)
    }

    /// Get raw memory pointer
    pub fn get_raw_ptr(&self) -> *mut u8 {
        let segments = self.segments.lock().unwrap();
        
        for segment in segments.values() {
            if segment.get_id() == self.segment_id {
                return segment.get_memory_ptr();
            }
        }
        
        std::ptr::null_mut()
    }

    /// Get segment size
    pub fn get_size(&self) -> usize {
        self.config.size
    }

    /// Get segment name
    pub fn get_name(&self) -> &str {
        &self.config.name
    }
}

impl Drop for SharedMemoryHandle {
    fn drop(&mut self) {
        let segment_id = self.segment_id;
        let should_remove = {
            let segments = self.segments.lock().unwrap();
            if let Some(segment) = segments.values().find(|s| s.get_id() == segment_id) {
                let mut ref_count = segment.ref_count.lock().unwrap();
                *ref_count = ref_count.saturating_sub(1);
                *ref_count == 0
            } else {
                false
            }
        };
        
        if should_remove {
            let mut segments = self.segments.lock().unwrap();
            segments.retain(|_, s| s.get_id() != segment_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_memory_config() {
        let config = SharedMemoryConfig::new("test_shm", 4096)
            .with_permissions(SharedMemoryPermissions::ReadWrite)
            .with_alignment(128)
            .with_cache_optimization(false);

        assert_eq!(config.name, "test_shm");
        assert_eq!(config.size, 4096);
        assert_eq!(config.permissions, SharedMemoryPermissions::ReadWrite);
        assert_eq!(config.alignment, 128);
        assert!(!config.cache_optimized);
    }

    #[test]
    fn test_memory_region() {
        let data = vec![1u32, 2, 3, 4, 5];
        let region = MemoryRegion::new("test_region".to_string(), data);
        
        assert_eq!(region.size(), 20);
        assert_eq!(region.as_slice().len(), 5);
        
        unsafe {
            assert_eq!(*region.get(0), 1);
            assert_eq!(*region.get(4), 5);
            
            let slice = region.as_slice();
            assert_eq!(slice, &[1, 2, 3, 4, 5]);
        }
    }

    #[test]
    fn test_shared_memory_manager() {
        let manager = SharedMemoryManager::new().unwrap();
        
        let config = SharedMemoryConfig::new("test_segment", 1024);
        let handle = manager.create_segment(config).unwrap();
        
        assert_eq!(handle.get_name(), "test_segment");
        assert_eq!(handle.get_size(), 1024);
        
        let stats = manager.get_stats();
        assert_eq!(stats.total_segments, 1);
        assert!(stats.total_memory_bytes >= 1024);
    }

    #[test]
    fn test_error_handling() {
        let manager = SharedMemoryManager::new().unwrap();
        
        let config = SharedMemoryConfig::new("invalid", 0);
        assert!(manager.create_segment(config).is_err());
        
        let config = SharedMemoryConfig::new("duplicate", 1024);
        let _handle1 = manager.create_segment(config.clone()).unwrap();
        assert!(manager.create_segment(config).is_err());
        
        assert!(manager.open_segment("nonexistent").is_err());
    }
}