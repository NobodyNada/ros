use crate::{prelude::*, util::Global, x86::mmu};

/// Allocates a physical page.
/// Returns the address of the page, or None if the system is out of memory.
pub fn palloc() -> Option<usize> {
    ALLOCATOR.take().unwrap().alloc()
}

pub struct PhysAllocator {
    bump_allocator: BumpAllocator,
}
static ALLOCATOR: Global<PhysAllocator> = Global::lazy(|| unsafe { PhysAllocator::new() });

extern "C" {
    /// The memory map obtained from the BIOS in boot.asm.
    /// NOTE: This is a physical address, not a virtual address!
    static mut PHYS_MEMORY_MAP: [MemoryRegion; 0];

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

impl PhysAllocator {
    unsafe fn new() -> Self {
        PhysAllocator {
            bump_allocator: BumpAllocator::new(),
        }
    }

    /// Allocates a page of physical memory, and returns its address.
    pub fn alloc(&mut self) -> Option<usize> {
        self.bump_allocator.alloc()
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
    pub unsafe fn new() -> Self {
        // Relocate the memory map pointer
        let memory_map_ptr = ((PHYS_MEMORY_MAP.as_mut_ptr() as usize)
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
        result.next_addr = result.find_next(core::ptr::addr_of_mut!(PHYSALLOC_START) as usize);
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
