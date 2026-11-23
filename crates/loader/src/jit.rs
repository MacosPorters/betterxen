// SPDX-License-Identifier: GPL-3.0-or-later

//! W^X-compliant native executable JIT memory allocator for Apple Silicon.
//!
//! Uses macOS `MAP_JIT` with `pthread_jit_write_protect_np` and cache invalidation.

use std::fmt;
use std::ptr::NonNull;

unsafe extern "C" {
    fn pthread_jit_write_protect_np(enabled: i32);
    fn sys_dcache_flush(start: *mut libc::c_void, len: usize);
    fn sys_icache_invalidate(start: *mut libc::c_void, len: usize);
}

/// Errors relating to JIT memory allocation and protection.
#[derive(Debug, PartialEq, Eq)]
pub enum JitError {
    AllocationFailed(i32),
    CapacityExceeded,
    ProtectionFailed,
}

impl fmt::Display for JitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllocationFailed(err) => write!(f, "mmap MAP_JIT failed with errno {err}"),
            Self::CapacityExceeded => write!(f, "JIT code cache capacity exceeded"),
            Self::ProtectionFailed => write!(f, "failed to alter JIT memory protection"),
        }
    }
}

impl std::error::Error for JitError {}

/// W^X-compliant JIT code buffer on macOS ARM64.
pub struct JitMemory {
    ptr: NonNull<u8>,
    capacity: usize,
    used: usize,
}

// JitMemory manages thread-independent JIT code memory (functions can be invoked across threads)
unsafe impl Send for JitMemory {}
unsafe impl Sync for JitMemory {}

impl JitMemory {
    /// Allocate a new JIT memory region using `MAP_JIT`.
    pub fn new(capacity: usize) -> Result<Self, JitError> {
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize };
        let aligned_capacity = (capacity + page_size - 1) & !(page_size - 1);

        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                aligned_capacity,
                libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC,
                libc::MAP_ANON | libc::MAP_PRIVATE | libc::MAP_JIT,
                -1,
                0,
            )
        };

        if ptr == libc::MAP_FAILED {
            let errno = unsafe { *libc::__error() };
            return Err(JitError::AllocationFailed(errno));
        }

        // Initialize to executable state by default (write-protected)
        unsafe {
            pthread_jit_write_protect_np(1);
        }

        Ok(Self {
            ptr: NonNull::new(ptr as *mut u8).unwrap(),
            capacity: aligned_capacity,
            used: 0,
        })
    }

    /// Base pointer of the JIT allocation.
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }

    /// Total capacity in bytes.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Currently committed bytes.
    pub fn used(&self) -> usize {
        self.used
    }

    /// Emit an ARM64 machine code block into the JIT memory.
    ///
    /// Automatically manages `pthread_jit_write_protect_np`, copies bytes,
    /// flushes data cache and invalidates instruction cache.
    pub fn emit_block(&mut self, instructions: &[u32]) -> Result<*const u8, JitError> {
        let bytes_len = instructions.len() * 4;
        if self.used + bytes_len > self.capacity {
            return Err(JitError::CapacityExceeded);
        }

        let emit_offset = self.used;
        let dst_ptr = unsafe { self.ptr.as_ptr().add(emit_offset) };

        unsafe {
            // 1. Enable write permission for this thread
            pthread_jit_write_protect_np(0);

            // 2. Copy machine instructions
            std::ptr::copy_nonoverlapping(instructions.as_ptr() as *const u8, dst_ptr, bytes_len);

            // 3. Flush D-cache and invalidate I-cache
            sys_dcache_flush(dst_ptr as *mut libc::c_void, bytes_len);
            sys_icache_invalidate(dst_ptr as *mut libc::c_void, bytes_len);

            // 4. Restore write-protection (executable only)
            pthread_jit_write_protect_np(1);
        }

        self.used += bytes_len;
        Ok(dst_ptr as *const u8)
    }

    /// Clear the JIT cache buffer for reuse.
    pub fn clear(&mut self) {
        self.used = 0;
    }
}

impl Drop for JitMemory {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.as_ptr() as *mut libc::c_void, self.capacity);
        }
    }
}
