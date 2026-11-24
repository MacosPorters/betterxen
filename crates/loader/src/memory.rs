// SPDX-License-Identifier: GPL-3.0-or-later

//! Guest virtual memory model for Xenora's x86-64 execution bootstrap.
//!
//! Provides isolated, page-aligned guest virtual memory with permission checking
//! (Read, Write, Execute) and bounds enforcement without host OS mapping tricks.

use std::fmt;

pub(crate) const ALLOCATION_GRANULARITY: u64 = 65_536;

/// Permission flags for a mapped memory region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryPermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl MemoryPermissions {
    pub const NO_ACCESS: Self = Self {
        read: false,
        write: false,
        execute: false,
    };
    pub const READ_ONLY: Self = Self {
        read: true,
        write: false,
        execute: false,
    };
    pub const READ_WRITE: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const READ_EXECUTE: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
    pub const READ_WRITE_EXECUTE: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
}

/// Errors occurring during guest memory operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryError {
    UnmappedAddress(u64),
    PermissionViolation {
        address: u64,
        required: &'static str,
    },
    OverlappingRegion {
        base: u64,
        size: u64,
    },
    Overflow,
    AllocationFailed,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnmappedAddress(addr) => write!(f, "unmapped guest address 0x{addr:016x}"),
            Self::PermissionViolation { address, required } => {
                write!(
                    f,
                    "permission violation at guest address 0x{address:016x}: requires {required}"
                )
            }
            Self::OverlappingRegion { base, size } => {
                write!(
                    f,
                    "overlapping memory region: base 0x{base:016x}, size 0x{size:x}"
                )
            }
            Self::Overflow => write!(f, "guest memory address calculation overflow"),
            Self::AllocationFailed => write!(f, "guest memory allocation exceeds allowed limit"),
        }
    }
}

impl std::error::Error for MemoryError {}

pub(crate) const PROCESS_HEAP: u64 = u64::MAX - 2;
const MAX_DYNAMIC_BYTES: usize = 64 * 1024 * 1024;
/// Host storage bound for each dynamic allocation kind.
pub(crate) const DYNAMIC_BUDGET: u64 = MAX_DYNAMIC_BYTES as u64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum AllocationKind {
    ImageOrStatic,
    Virtual,
    Heap(u64),
}

#[derive(Clone)]
struct MappedRegion {
    base: u64,
    data: Vec<u8>,
    permissions: MemoryPermissions,
    allocation: AllocationKind,
}

impl MappedRegion {
    fn contains(&self, address: u64, size: usize) -> bool {
        let size_u64 = size as u64;
        let Some(end) = address.checked_add(size_u64) else {
            return false;
        };
        let region_end = self.base + self.data.len() as u64;
        address >= self.base && end <= region_end
    }
}

/// Guest virtual memory address space.
#[derive(Default, Clone)]
pub struct GuestMemory {
    regions: Vec<MappedRegion>,
    private_heaps: std::collections::BTreeSet<u64>,
    private_heap_serial: u64,
    // Stable errno pointers indexed by the guest thread's TEB address.
    crt_errno: std::collections::BTreeMap<u64, u64>,
    pub(crate) crt_token_cursors: std::collections::BTreeMap<u64, u64>,
    pub(crate) callback_frames:
        std::collections::BTreeMap<(u64, u64), crate::imports::CallbackFrame>,
}

impl GuestMemory {
    /// Create a new empty guest address space.
    pub fn new() -> Self {
        Self::default()
    }

    /// Map a new region into guest virtual memory.
    pub fn map_region(
        &mut self,
        base: u64,
        size: usize,
        permissions: MemoryPermissions,
    ) -> Result<(), MemoryError> {
        self.map_region_with_data(base, size, permissions, &[])
    }

    /// Map a new region and initialize it with initial data, bypassing runtime write permission checks.
    pub fn map_region_with_data(
        &mut self,
        base: u64,
        size: usize,
        permissions: MemoryPermissions,
        initial_data: &[u8],
    ) -> Result<(), MemoryError> {
        self.insert_region(
            base,
            size,
            permissions,
            initial_data,
            AllocationKind::ImageOrStatic,
        )
    }

    fn insert_region(
        &mut self,
        base: u64,
        size: usize,
        permissions: MemoryPermissions,
        initial_data: &[u8],
        allocation: AllocationKind,
    ) -> Result<(), MemoryError> {
        let size_u64 = size as u64;
        let end = base.checked_add(size_u64).ok_or(MemoryError::Overflow)?;
        for r in &self.regions {
            let r_end = r.base + r.data.len() as u64;
            if base < r_end && r.base < end {
                return Err(MemoryError::OverlappingRegion {
                    base,
                    size: size_u64,
                });
            }
        }

        let mut data = Vec::new();
        data.try_reserve_exact(size)
            .map_err(|_| MemoryError::AllocationFailed)?;
        data.resize(size, 0);
        let copy_len = size.min(initial_data.len());
        data[..copy_len].copy_from_slice(&initial_data[..copy_len]);

        self.regions.push(MappedRegion {
            base,
            data,
            permissions,
            allocation,
        });
        self.regions.sort_by_key(|r| r.base);
        Ok(())
    }

    pub(crate) fn map_virtual_allocation(
        &mut self,
        requested_base: u64,
        size: usize,
        permissions: MemoryPermissions,
    ) -> Result<u64, MemoryError> {
        self.map_dynamic_allocation(requested_base, size, permissions, AllocationKind::Virtual)
    }

    /// Bytes mapped in total, held by virtual allocations, and held by heap
    /// blocks. Each of the last two kinds is bounded by [`DYNAMIC_BUDGET`].
    pub(crate) fn usage(&self) -> (u64, u64, u64) {
        let mapped = self
            .regions
            .iter()
            .map(|region| region.data.len())
            .sum::<usize>();
        let virtual_bytes = self.dynamic_bytes(AllocationKind::Virtual);
        let heap_bytes = self.dynamic_bytes(AllocationKind::Heap(PROCESS_HEAP));
        (mapped as u64, virtual_bytes as u64, heap_bytes as u64)
    }

    pub(crate) fn crt_errno_address(&mut self, teb: u64) -> Result<u64, MemoryError> {
        if let Some(&address) = self.crt_errno.get(&teb) {
            return Ok(address);
        }
        if self.crt_errno.len() >= 1024 {
            return Err(MemoryError::AllocationFailed);
        }
        let address = self.find_dynamic_base(4)?;
        self.map_region(address, 4, MemoryPermissions::READ_WRITE)?;
        self.crt_errno.insert(teb, address);
        Ok(address)
    }

    pub(crate) fn create_private_heap(&mut self, initial: u64) -> Option<u64> {
        if initial > MAX_DYNAMIC_BYTES as u64
            || self.private_heaps.len() >= 256
            || self.private_heap_serial >= i64::MAX as u64 - 3
        {
            return None;
        }
        self.private_heap_serial += 1;
        let heap = PROCESS_HEAP - self.private_heap_serial;
        self.private_heaps.insert(heap);
        Some(heap)
    }

    pub(crate) fn has_heap(&self, heap: u64) -> bool {
        heap == PROCESS_HEAP || self.private_heaps.contains(&heap)
    }

    pub(crate) fn destroy_private_heap(&mut self, heap: u64) -> bool {
        if !self.private_heaps.remove(&heap) {
            return false;
        }
        self.regions
            .retain(|region| region.allocation != AllocationKind::Heap(heap));
        true
    }

    pub(crate) fn map_heap_allocation(
        &mut self,
        heap: u64,
        size: usize,
    ) -> Result<u64, MemoryError> {
        if !self.has_heap(heap) {
            return Err(MemoryError::AllocationFailed);
        }
        self.map_dynamic_allocation(
            0,
            size,
            MemoryPermissions::READ_WRITE,
            AllocationKind::Heap(heap),
        )
    }

    fn map_dynamic_allocation(
        &mut self,
        requested_base: u64,
        size: usize,
        permissions: MemoryPermissions,
        allocation: AllocationKind,
    ) -> Result<u64, MemoryError> {
        // Bound retained host storage independently for each allocation kind.
        let allocated = self.dynamic_bytes(allocation);
        if size > MAX_DYNAMIC_BYTES - allocated
            || (matches!(allocation, AllocationKind::Heap(_))
                && self
                    .regions
                    .iter()
                    .filter(|r| matches!(r.allocation, AllocationKind::Heap(_)))
                    .count()
                    >= 4096)
        {
            return Err(MemoryError::AllocationFailed);
        }

        let base = if requested_base == 0 {
            self.find_dynamic_base(size)?
        } else {
            requested_base
        };
        self.insert_region(base, size, permissions, &[], allocation)?;
        Ok(base)
    }

    fn dynamic_bytes(&self, allocation: AllocationKind) -> usize {
        self.regions
            .iter()
            .filter(|region| match allocation {
                AllocationKind::Heap(_) => matches!(region.allocation, AllocationKind::Heap(_)),
                _ => region.allocation == allocation,
            })
            .map(|region| region.data.len())
            .sum()
    }

    fn find_dynamic_base(&self, size: usize) -> Result<u64, MemoryError> {
        let mut base = 0x0000_0003_0000_0000u64;
        for region in &self.regions {
            let end = base.checked_add(size as u64).ok_or(MemoryError::Overflow)?;
            if end <= region.base {
                return Ok(base);
            }
            let region_end = region.base + region.data.len() as u64;
            if base < region_end {
                base = region_end
                    .checked_next_multiple_of(ALLOCATION_GRANULARITY)
                    .ok_or(MemoryError::Overflow)?;
            }
        }
        base.checked_add(size as u64).ok_or(MemoryError::Overflow)?;
        Ok(base)
    }

    pub(crate) fn resize_heap_allocation(
        &mut self,
        heap: u64,
        base: u64,
        size: usize,
        in_place_only: bool,
    ) -> Result<u64, MemoryError> {
        let index = self
            .regions
            .iter()
            .position(|region| {
                region.base == base && region.allocation == AllocationKind::Heap(heap)
            })
            .ok_or(MemoryError::UnmappedAddress(base))?;
        let previous = self.regions[index].data.len();
        let other_bytes = self.dynamic_bytes(AllocationKind::Heap(heap)) - previous;
        if size > MAX_DYNAMIC_BYTES - other_bytes {
            return Err(MemoryError::AllocationFailed);
        }

        let boundary = self
            .regions
            .get(index + 1)
            .map_or(u64::MAX, |next| next.base);
        let destination = if (size as u64) <= boundary - base {
            base
        } else if in_place_only {
            return Err(MemoryError::AllocationFailed);
        } else {
            self.find_dynamic_base(size)?
        };

        let region = &mut self.regions[index];
        region
            .data
            .try_reserve_exact(size.saturating_sub(previous))
            .map_err(|_| MemoryError::AllocationFailed)?;
        region.data.resize(size, 0);
        if size < previous {
            region.data.shrink_to_fit();
        }
        if destination != base {
            region.base = destination;
            self.regions.sort_by_key(|region| region.base);
        }
        Ok(destination)
    }

    pub(crate) fn release_virtual_allocation(&mut self, base: u64) -> bool {
        self.release_dynamic_allocation(base, AllocationKind::Virtual)
    }

    pub(crate) fn release_heap_allocation(&mut self, heap: u64, base: u64) -> bool {
        self.release_dynamic_allocation(base, AllocationKind::Heap(heap))
    }

    pub(crate) fn heap_allocation_size(&self, heap: u64, base: u64) -> Option<usize> {
        self.regions
            .iter()
            .find(|region| region.base == base && region.allocation == AllocationKind::Heap(heap))
            .map(|region| region.data.len())
    }

    fn release_dynamic_allocation(&mut self, base: u64, allocation: AllocationKind) -> bool {
        let Some(index) = self
            .regions
            .iter()
            .position(|region| region.base == base && region.allocation == allocation)
        else {
            return false;
        };
        self.regions.remove(index);
        true
    }

    pub(crate) fn check_write(&self, address: u64, size: usize) -> Result<(), MemoryError> {
        self.resolve(address, size, Some("write")).map(|_| ())
    }

    /// Resolve `[address, address + len)` to `(region index, offset, length)` pieces.
    ///
    /// An access may span adjacent regions, as it would span adjacent pages, but every
    /// byte must be mapped and every region must grant `required` (`None` bypasses
    /// permission checks for the loader). The whole range is checked before any byte
    /// is copied, so a failed access never partially reads or writes.
    fn resolve(
        &self,
        address: u64,
        len: usize,
        required: Option<&'static str>,
    ) -> Result<Vec<(usize, usize, usize)>, MemoryError> {
        let end = address
            .checked_add(len as u64)
            .ok_or(MemoryError::Overflow)?;
        if len == 0 {
            return if self.regions.iter().any(|r| r.contains(address, 0)) {
                Ok(Vec::new())
            } else {
                Err(MemoryError::UnmappedAddress(address))
            };
        }
        let mut pieces = Vec::new();
        let mut cursor = address;
        for (index, region) in self.regions.iter().enumerate() {
            let region_end = region.base + region.data.len() as u64;
            if region_end <= cursor {
                continue;
            }
            if region.base > cursor {
                break;
            }
            let permitted = match required {
                None => true,
                Some("read") => region.permissions.read,
                Some("write") => region.permissions.write,
                Some(_) => region.permissions.execute,
            };
            if let (false, Some(required)) = (permitted, required) {
                return Err(MemoryError::PermissionViolation { address, required });
            }
            let take = (end.min(region_end) - cursor) as usize;
            pieces.push((index, (cursor - region.base) as usize, take));
            cursor += take as u64;
            if cursor == end {
                return Ok(pieces);
            }
        }
        Err(MemoryError::UnmappedAddress(address))
    }

    fn read_checked(
        &self,
        address: u64,
        dst: &mut [u8],
        required: Option<&'static str>,
    ) -> Result<(), MemoryError> {
        let mut done = 0;
        for (index, offset, take) in self.resolve(address, dst.len(), required)? {
            dst[done..done + take]
                .copy_from_slice(&self.regions[index].data[offset..offset + take]);
            done += take;
        }
        Ok(())
    }

    fn write_checked(
        &mut self,
        address: u64,
        src: &[u8],
        required: Option<&'static str>,
    ) -> Result<(), MemoryError> {
        let mut done = 0;
        for (index, offset, take) in self.resolve(address, src.len(), required)? {
            self.regions[index].data[offset..offset + take]
                .copy_from_slice(&src[done..done + take]);
            done += take;
        }
        Ok(())
    }

    /// Write raw bytes into guest memory at the specified address, checking permissions.
    pub fn write_bytes(&mut self, address: u64, src: &[u8]) -> Result<(), MemoryError> {
        self.write_checked(address, src, Some("write"))
    }

    /// Read raw bytes from guest memory at the specified address, checking permissions.
    pub fn read_bytes(&self, address: u64, dst: &mut [u8]) -> Result<(), MemoryError> {
        self.read_checked(address, dst, Some("read"))
    }

    /// Read an instruction byte from guest memory, verifying execute permission.
    pub fn fetch_instruction_byte(&self, address: u64) -> Result<u8, MemoryError> {
        let mut buf = [0u8; 1];
        self.read_checked(address, &mut buf, Some("execute"))?;
        Ok(buf[0])
    }

    /// Read a 64-bit integer (little endian) from guest memory.
    pub fn read_u64(&self, address: u64) -> Result<u64, MemoryError> {
        let mut buf = [0u8; 8];
        self.read_bytes(address, &mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Read a 64-bit integer ignoring region read permissions (e.g. loader/relocation phase).
    pub fn read_u64_bypass(&self, address: u64) -> Result<u64, MemoryError> {
        let mut buf = [0u8; 8];
        self.read_checked(address, &mut buf, None)?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Write a 64-bit integer (little endian) to guest memory.
    pub fn write_u64(&mut self, address: u64, value: u64) -> Result<(), MemoryError> {
        self.write_bytes(address, &value.to_le_bytes())
    }

    /// Write a 64-bit integer ignoring region write permissions (e.g. loader/relocation phase).
    pub fn write_u64_bypass(&mut self, address: u64, value: u64) -> Result<(), MemoryError> {
        self.write_checked(address, &value.to_le_bytes(), None)
    }

    /// Read a 32-bit integer (little endian) from guest memory.
    pub fn read_u32(&self, address: u64) -> Result<u32, MemoryError> {
        let mut buf = [0u8; 4];
        self.read_bytes(address, &mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    /// Write a 32-bit integer (little endian) to guest memory.
    pub fn write_u32(&mut self, address: u64, value: u32) -> Result<(), MemoryError> {
        self.write_bytes(address, &value.to_le_bytes())
    }

    /// Write a 32-bit integer ignoring region write permissions (e.g. loader/relocation phase).
    pub fn write_u32_bypass(&mut self, address: u64, value: u32) -> Result<(), MemoryError> {
        self.write_checked(address, &value.to_le_bytes(), None)
    }

    /// Read a single byte from guest memory.
    pub fn read_u8(&self, address: u64) -> Result<u8, MemoryError> {
        let mut buf = [0u8; 1];
        self.read_bytes(address, &mut buf)?;
        Ok(buf[0])
    }

    /// Write a single byte to guest memory.
    pub fn write_u8(&mut self, address: u64, value: u8) -> Result<(), MemoryError> {
        self.write_bytes(address, &[value])
    }

    /// Read a 16-bit little-endian integer from guest memory.
    pub fn read_u16(&self, address: u64) -> Result<u16, MemoryError> {
        let mut buf = [0u8; 2];
        self.read_bytes(address, &mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    /// Write a 16-bit little-endian integer to guest memory.
    pub fn write_u16(&mut self, address: u64, value: u16) -> Result<(), MemoryError> {
        self.write_bytes(address, &value.to_le_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_heap_destruction_reclaims_only_owned_regions() {
        let mut memory = GuestMemory::new();
        let private = memory.create_private_heap(0).unwrap();
        let peer = memory.create_private_heap(0).unwrap();
        let address = memory.map_heap_allocation(private, 16).unwrap();
        let retained = memory.map_heap_allocation(peer, 16).unwrap();
        assert!(!memory.release_heap_allocation(peer, address));
        assert_eq!(memory.heap_allocation_size(PROCESS_HEAP, address), None);
        assert!(!memory.destroy_private_heap(PROCESS_HEAP));
        assert!(memory.destroy_private_heap(private));
        assert!(!memory.destroy_private_heap(private));
        assert!(memory.map_heap_allocation(private, 16).is_err());
        assert!(memory.read_u8(address).is_err());
        assert_eq!(memory.read_u8(retained), Ok(0));
        assert_ne!(memory.create_private_heap(0).unwrap(), private);
    }

    #[test]
    fn heap_resize_moves_only_when_allowed_and_preserves_bytes() {
        let mut memory = GuestMemory::new();
        let base = memory.map_heap_allocation(PROCESS_HEAP, 19).unwrap();
        let neighbor = memory.map_heap_allocation(PROCESS_HEAP, 1).unwrap();
        memory.write_u8(base, 0xab).unwrap();
        memory.write_u8(neighbor, 0xcd).unwrap();
        let growth = (neighbor - base) as usize + 1;
        assert_eq!(
            memory.resize_heap_allocation(PROCESS_HEAP, base, growth, true),
            Err(MemoryError::AllocationFailed)
        );
        assert_eq!(memory.heap_allocation_size(PROCESS_HEAP, base), Some(19));
        let moved = memory
            .resize_heap_allocation(PROCESS_HEAP, base, growth, false)
            .unwrap();
        assert_ne!(moved, base);
        assert_eq!(memory.read_u8(moved), Ok(0xab));
        assert_eq!(memory.read_u8(neighbor), Ok(0xcd));
        assert_eq!(memory.read_u8(moved + growth as u64 - 1), Ok(0));
        assert_eq!(memory.heap_allocation_size(PROCESS_HEAP, base), None);
        assert_eq!(
            memory.resize_heap_allocation(PROCESS_HEAP, moved, 1, true),
            Ok(moved)
        );
        assert_eq!(memory.heap_allocation_size(PROCESS_HEAP, moved), Some(1));
    }
}
