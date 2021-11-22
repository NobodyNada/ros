#![allow(clippy::identity_op)]
#![allow(dead_code)]
use core::num::NonZeroUsize;

use crate::{prelude::*, x86::mmu};
use modular_bitfield::prelude::*;

use super::mmap::MemoryMapper;

pub struct PhysAllocator {
    bump_allocator: BumpAllocator,
    max_allocated: usize,

    /// The physical memory address of the next free page
    /// (NOTE: NOT the PhysPageInfo struct describing said page)
    freelist_head: Option<NonZeroUsize>,
}

extern "C" {
    /// The memory map obtained from the BIOS in boot.asm.
    /// NOTE: This is a physical address, not a virtual address!
    static mut BIOS_MEMORY_MAP: [MemoryRegion; 0];

    static mut PHYSALLOC_START: u8;
}

#[repr(C)]
#[derive(Debug)]
struct MemoryRegion {
    start: u64,
    length: u64,
    kind: u32,
    attributes: u32,
}

impl MemoryRegion {
    fn should_ignore(&self) -> bool {
        self.attributes & 1 == 0
    }

    fn is_usable(&self) -> bool {
        self.kind == 1
    }

    /// Returns true iff every address in this region
    /// comes after every address in the given region.
    fn is_after(&self, start: usize, length: usize) -> bool {
        let s = self.start as usize;
        s > start && length <= (s - start)
    }

    /// Returns true iff every address in this region
    /// comes before start.
    fn is_before(&self, start: usize) -> bool {
        let s = self.start as usize;
        let l = self.length as usize;
        start > s && l <= (start - s)
    }
}

#[derive(Clone, Copy)]
pub union PhysPageInfo {
    pub allocated: AllocatedPageInfo,

    /// The physical address of the next free page
    /// (NOT a pointer to the next free PhysPageInfo struct)
    pub free: Option<NonZeroUsize>,
}

impl Default for PhysPageInfo {
    fn default() -> Self {
        // zero is a valid default representation for both 'allocated' and 'free'
        Self { free: None }
    }
}

#[bitfield]
#[repr(u32)]
#[derive(Clone, Copy, Default)]
pub struct AllocatedPageInfo {
    pub refcount: B31,
    pub copy_on_write: bool,
}

impl PhysAllocator {
    pub(super) unsafe fn new() -> Self {
        let start = core::ptr::addr_of_mut!(PHYSALLOC_START) as usize;
        PhysAllocator {
            bump_allocator: BumpAllocator::new(start),
            max_allocated: mmu::page_align_down(start - 1),
            freelist_head: None,
        }
    }

    /// Allocates a page of physical memory, and returns its address.
    /// This function is deliberately very simple, and is designed to work without needing to
    /// access the PhysPageInfo structures so that it can work even before memory mappings have
    /// been properly initialized.
    pub fn alloc(&mut self) -> Option<usize> {
        if let Some(paddr) = self.freelist_head {
            let paddr = paddr.get();
            unsafe {
                // SAFETY: normally we can't assume a pageinfo is writable without calling
                // mapper.cow_if_needed. However, in this case, the pointer has already been
                // written to before (when it was added to the freelist), and there is no way for
                // a PhysPageInfo to become COW again.
                let info = self.get_page_info(paddr) as *mut PhysPageInfo;
                self.freelist_head = (*info).free;
                *info = PhysPageInfo::default();
            }
            Some(paddr)
        } else {
            let result = self.bump_allocator.alloc()?;
            self.max_allocated = result;
            Some(result)
        }
    }

    /// Increments the refcount of a physical page.
    ///
    /// # Safety
    ///
    /// The caller is responsible for ensuring the page was allocated.
    pub unsafe fn share(&mut self, paddr: usize, mapper: &mut MemoryMapper) {
        let paddr = mmu::page_align_down(paddr);
        let info = self.get_page_info_mut(paddr, mapper).as_mut().unwrap();
        info.allocated.set_refcount(info.allocated.refcount() + 1);
    }

    /// Marks a page as copy-on-write and increments the refcount.
    /// The page's refcount must have formerly been zero, because it is not possible to track down
    /// all the existing references to the page.
    /// Note that this function takes a virtual address, not a physical address, because it has to
    /// update mappings.
    ///
    /// # Safety
    ///
    /// The caller is responsible for ensuring the page was in fact allocated.
    pub unsafe fn share_vaddr_cow(&mut self, vaddr: usize, mapper: &mut MemoryMapper) {
        let mapping = mapper.get_mapping(vaddr).expect("page is not mapped");

        let paddr = mapping.physaddr() as usize;
        let info = self.get_page_info_mut(paddr, mapper).as_mut().unwrap();
        assert!(
            info.allocated.copy_on_write() || info.allocated.refcount() == 0,
            "cannot mark a shared page as copy-on-write"
        );

        info.allocated.set_copy_on_write(true);
        info.allocated.set_refcount(info.allocated.refcount() + 1);

        // Mark the page as read-only
        mapper.map(
            self,
            paddr,
            vaddr,
            mapper.mapping_to_flags(mapping).with_writable(false),
        );
    }

    /// Decrements the reference count of a page of memory, freeing the page
    /// if the reference count reaches zero.
    ///
    /// # Safety
    ///
    /// The caller is responsible for ensuring the page was in fact allocated.
    pub unsafe fn free(&mut self, paddr: usize, mapper: &mut MemoryMapper) {
        assert!(
            paddr >= core::ptr::addr_of!(PHYSALLOC_START) as usize,
            "free: paddr {:#08x} < PHYSALLOC_START",
            paddr
        );

        let paddr = mmu::page_align_down(paddr);
        let info = self.get_page_info_mut(paddr, mapper).as_mut().unwrap();

        if info.allocated.refcount() == 0 {
            *info = PhysPageInfo {
                free: self.freelist_head,
            };
            self.freelist_head =
                Some(NonZeroUsize::new(paddr).expect("attempt to free a null pointer"))
        } else {
            info.allocated.set_refcount(info.allocated.refcount() - 1);
        }
    }

    /// Returns the highest physical memory address that has been allocated at some point.
    /// Used when setting up virtual memory in order to determine how far we need to create
    /// identity mappings for early allocations.
    pub fn get_max_allocated(&self) -> usize {
        self.max_allocated
    }

    /// Returns a pointer to the page info for a given physical address.
    /// The returned pointer is const because it may be mapped as copy-on-write,
    /// and accessing copy-on-write pages is not safe within the physical allocator.
    ///
    /// To get a mutable pointer, use get_page_info_ptr_mut.
    pub fn get_page_info(&self, paddr: usize) -> *const PhysPageInfo {
        assert!(paddr != 0, "physical address is null");
        unsafe { (mmu::mmap::PAGEINFO_BASE as *mut PhysPageInfo).add(paddr / mmu::PAGE_SIZE) }
    }

    /// Returns a mutable pointer to the page info for a given physical address.
    pub fn get_page_info_mut(
        &mut self,
        paddr: usize,
        mapper: &mut MemoryMapper,
    ) -> *mut PhysPageInfo {
        let ptr = self.get_page_info(paddr);
        mapper.cow_if_needed(self, ptr as usize);
        ptr as *mut _
    }
}

/// A simple bump allocator that allocates physical memory pages in order from the map, with no
/// support for freeing memory.
struct BumpAllocator {
    next_addr: Option<usize>,
    first_active_region_idx: usize,
    memory_map: &'static [MemoryRegion],
}

impl BumpAllocator {
    pub unsafe fn new(start_addr: usize) -> Self {
        // Relocate the memory map pointer
        let memory_map_ptr = ((BIOS_MEMORY_MAP.as_mut_ptr() as usize)
            + mmu::KERNEL_RELOC_BASE as usize) as *mut MemoryRegion;
        let map_count = (0..)
            .find(|&i| (*memory_map_ptr.add(i)).length == 0)
            .unwrap();

        let memory_map = core::slice::from_raw_parts_mut(memory_map_ptr, map_count);
        // Sort the memory map in place
        memory_map.sort_unstable_by_key(|r| r.start);

        kprintln!("Physical memory map:");
        kprintln!("{:#08x?}", memory_map);

        let mut result = Self {
            next_addr: None,
            first_active_region_idx: 0,
            memory_map,
        };
        result.next_addr = result.find_next(start_addr);
        result
    }

    pub fn alloc(&mut self) -> Option<usize> {
        let result = self.next_addr?;
        self.next_addr = self.find_next(result + mmu::PAGE_SIZE);
        Some(result)
    }

    /// If `addr` does not lie within a usable memory region, rounds it up to the next usable
    /// region. Returns `None` if no more usable memory is available beyond `addr`.
    ///
    /// Always returns a page-aligned address.
    fn find_next(&mut self, mut addr: usize) -> Option<usize> {
        addr = mmu::page_align_up(addr)?; // if this overflows, we're out of usable memory

        let mut is_usable = false;
        while !is_usable {
            if self.first_active_region_idx >= self.memory_map.len() {
                // We've exhausted available memory.
                return None;
            }

            // Iterate over all regions that may contain the target address.
            let mut is_first = true;
            for region in self.memory_map.iter().skip(self.first_active_region_idx) {
                if !region.should_ignore() && region.is_after(addr, mmu::PAGE_SIZE) {
                    // We haven't yet reached the start of this region.
                    if is_first {
                        // There is no region containing the address, so we need to bump up the
                        // address to the start of this first region.
                        addr = mmu::page_align_up(region.start as usize)?;
                    } else {
                        // We've gone through all the regions containing the target; we're done.
                        break;
                    }
                }
                // If we're already past the end of this region, or we should ignore it, skip it.
                else if region.should_ignore() || region.is_before(addr) {
                    // Bump up the first active region if needed
                    if is_first {
                        self.first_active_region_idx += 1;
                    }
                    continue;
                }
                is_first = false;

                // The page intersects 'region'.
                if region.is_usable() {
                    // This region is marked as usable, but keep checking to make sure it's not
                    // marked as unusable by an overlapping region.
                    is_usable = true;
                } else {
                    // This region is marked as reserved.
                    is_usable = false;

                    // We can't possibly use any addresses within this region, so move the pointer
                    // to the end. If this overflows, we're out of usable memory
                    addr = mmu::page_align_up(
                        (region.start as usize).checked_add(region.length as usize)?,
                    )?;

                    // Try again at the new address.
                    break;
                }
            }
        }

        Some(addr)
    }
}
