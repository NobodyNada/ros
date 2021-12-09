//! ROS heap allocator
//!
//! A simple heap allocator, providing a malloc/free interface compatible with Rust's standard
//! library & allowing us to use collection & smart pointer types from the `alloc` standard library crate.
//!
//! For "big" allocations (greater than 2048 bytes, or half a page) we simply allocate whole pages
//! using the MMU. For "small" allocations, we use a buddy allocator to subdivide individual pages.
//!
//! Each page belonging to the small allocator begins with a 64-byte header, indicating which
//! blocks of that page are free vs in use. The header contains 9 arrays with one bit per element:
//! a 1-element array covering the whole page, a 2-element array covering 2048-byte blocks, a
//! 4-element array covering 1024-byte blocks and so on, down to a 256-element array covering
//! 16-byte blocks (the smallest allocation size). If a bit is set in the header, that means at
//! least one allocation exists in the region spanned by that bit.
//!
//! Free blocks are located using freelists. We maintain one freelist for each possible allocation
//! size, and anytime a block is free while its "buddy" is in use, the block is on a freelist.

use crate::mmu;
use crate::util::Global;
use core::{
    alloc::{AllocError, GlobalAlloc},
    ops::DerefMut,
};

macro_rules! debug {
    ($($arg:tt)*) => {
        //$crate::kprint!($($arg)*)
    };
}

macro_rules! debugln {
    ($($arg:tt)*) => {
        //$crate::kprintln!($($arg)*)
    };
}

pub const MAX_ALLOC: usize = mmu::PAGE_SIZE / 2;
pub const MIN_ALLOC: usize = 16;

// Create one freelist for every power-of-2 size from 4096, 2048, 1024, ..., 32, 16.
// (Note: the 4096-sized freelist is never used, because we use whole-page allocations for objects
// that large. However, the buddy allocator needs to be able to reason about 4096-sized blocks, so
// having an unused freelist for it reduces special cases.)
const NUM_FREELISTS: usize =
    MIN_ALLOC.leading_zeros() as usize - mmu::PAGE_SIZE.leading_zeros() as usize + 1;

pub struct HeapAllocator {
    allocator: crate::util::Global<_HeapAllocator>,
}

#[global_allocator]
static ALLOCATOR: HeapAllocator = HeapAllocator::new();

impl HeapAllocator {
    const fn new() -> Self {
        HeapAllocator {
            allocator: Global::lazy(_HeapAllocator::new),
        }
    }
}

unsafe impl GlobalAlloc for HeapAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        debugln!("alloc({:#08x?})", layout);
        self.allocator
            .take()
            .unwrap()
            .allocate(layout)
            .unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        self.allocator.take().unwrap().deallocate(ptr, layout);
    }
}

pub struct _HeapAllocator {
    freelists: [*mut FreelistEntry; NUM_FREELISTS],
}

impl _HeapAllocator {
    fn new() -> Self {
        _HeapAllocator {
            freelists: [core::ptr::null_mut(); NUM_FREELISTS],
        }
    }

    unsafe fn allocate(&mut self, layout: core::alloc::Layout) -> Result<*mut u8, AllocError> {
        // We can't align something to more than a page.
        if layout.align() > mmu::PAGE_SIZE {
            return Err(AllocError);
        }

        // Size and alignment are the same; use whichever is greater.
        let size = core::cmp::max(layout.size(), layout.align()).next_power_of_two();
        let size = core::cmp::max(size, MIN_ALLOC);
        let mut mmu = mmu::MMU.take().unwrap();
        let mmu = mmu.deref_mut();
        if size > MAX_ALLOC {
            // This is a big allocation; simply map some pages for it and we're done.
            debugln!("big");
            let pages = mmu::page_align_up(size).unwrap() / mmu::PAGE_SIZE;
            let vaddr = mmu
                .mapper
                .find_unused_kernelspace(pages)
                .ok_or(AllocError)?;
            debugln!("alloced at {:#08x}", vaddr);
            mmu.mapper.map_zeroed(
                &mut mmu.allocator,
                vaddr,
                pages,
                mmu::mmap::MappingFlags::new().with_writable(true),
            );
            Ok(vaddr as *mut u8)
        } else {
            // This is a small allocation; check the freelists first.
            debugln!("small");
            debugln!("Before: ");
            self.print_freelists();

            let freelist_idx =
                mmu::PAGE_SIZE.trailing_zeros() as usize - size.trailing_zeros() as usize;

            for i in (0..=freelist_idx).rev() {
                let head = &mut self.freelists[i];
                if head.is_null() {
                    continue;
                }

                // Use this freelist entry.
                let addr = *head;
                debugln!("\tusing freelist entry: {:#08x?}", addr);

                let page_start = mmu::page_align_down(addr as usize);
                let header = &mut *(page_start as *mut PageHeader);
                let idx = (addr as usize - page_start) / size;

                // Remove the entry from the freelist
                *head = (*addr).next;

                debugln!("\tpage header:");
                debugln!("\tbefore: {:?}", header);

                // Mark it as allocated
                header.alloc(self, idx, freelist_idx);

                debugln!("\tafter: {:?}", header);
                self.print_freelists();

                return Ok(addr as *mut u8);
            }

            // The freelists are emtpy; allocate a new page.
            debugln!("\tno freelist");
            let paddr = mmu.allocator.alloc().ok_or(AllocError)?;
            let vaddr = mmu.mapper.find_unused_kernelspace(1).ok_or(AllocError)?;
            debugln!("\talloced at {:#08x} -> {:#08x}", vaddr, paddr);
            mmu.mapper.map(
                &mut mmu.allocator,
                paddr,
                vaddr,
                mmu::mmap::MappingFlags::new().with_writable(true),
            );
            debugln!("mapped");

            // Initialize the page header
            let header = vaddr as *mut core::mem::MaybeUninit<PageHeader>;
            let header = (*header).write(Default::default());
            debugln!("wrote header: {:?}", header);

            // Mark a block as allocated
            // (allocate a block towards the end of the page so we don't have to worry about
            // intersecting the page header)
            let idx = (1 << freelist_idx) - 1;
            header.alloc(self, idx, freelist_idx);

            debugln!("\tpage header: {:?}", header);
            debugln!("After: ");
            self.print_freelists();

            // Return the virtual address that we just allocated
            Ok((vaddr + idx * size) as *mut u8)
        }
    }

    unsafe fn deallocate(&mut self, ptr: *mut u8, layout: core::alloc::Layout) {
        debugln!("deallocate({:#08x?}, {:#08x?})", ptr, layout);
        debugln!("before:");
        self.print_freelists();
        let size = core::cmp::max(layout.size(), layout.align()).next_power_of_two();
        let size = core::cmp::max(size, MIN_ALLOC);
        let vaddr = ptr as usize;
        let mut mmu = mmu::MMU.take().unwrap();
        let mmu = mmu.deref_mut();
        if size > MAX_ALLOC {
            let pages = mmu::page_align_up(size).unwrap() / mmu::PAGE_SIZE;
            for page_idx in 0..pages {
                let paddr = mmu
                    .mapper
                    .get_mapping(vaddr + page_idx * mmu::PAGE_SIZE)
                    .unwrap()
                    .physaddr() as usize;
                mmu.allocator.free(paddr, &mut mmu.mapper);
                mmu.mapper
                    .unmap(&mut mmu.allocator, vaddr + page_idx * mmu::PAGE_SIZE);
            }
        } else {
            let page_start = mmu::page_align_down(vaddr);
            let header = page_start as *mut PageHeader;
            let freelist_idx =
                mmu::PAGE_SIZE.trailing_zeros() as usize - size.trailing_zeros() as usize;
            let idx = (vaddr - page_start) / size;
            if (*header).free(self, idx, freelist_idx) {
                // There are no longer any allocations on that page.
                let paddr = mmu.mapper.get_mapping(page_start).unwrap().physaddr() as usize;
                mmu.allocator.free(paddr, &mut mmu.mapper);
                mmu.mapper.unmap(&mut mmu.allocator, page_start);
                debugln!("unmapped");
            }
        }
        debugln!("after:");
        self.print_freelists();
    }

    fn print_freelists(&self) {
        unsafe {
            debugln!("Freelists:");
            for mut ptr in self.freelists {
                while !ptr.is_null() {
                    debug!(" -> {:?}", ptr);
                    if !(*ptr).next.is_null() {
                        assert_eq!((*(*ptr).next).prev, ptr);
                    }
                    ptr = (*ptr).next;
                }
                debugln!("");
            }
        }
    }
}

struct FreelistEntry {
    prev: *mut FreelistEntry,
    next: *mut FreelistEntry,
}

struct PageHeader {
    blockinfo: [u8; (1 << (NUM_FREELISTS)) / 8],
}

impl Default for PageHeader {
    fn default() -> Self {
        PageHeader {
            blockinfo: [0; (1 << (NUM_FREELISTS)) / 8],
        }
    }
}

impl PageHeader {
    fn get_idx(&self, idx: usize, depth: usize) -> usize {
        assert!(depth < NUM_FREELISTS, "depth {} out of range", depth);
        assert!(
            idx < (1 << depth),
            "index {} out of range for depth {}",
            idx,
            depth
        );
        (1 << depth) + idx
    }

    fn is_block_in_use(&self, idx: usize, depth: usize) -> bool {
        let idx = self.get_idx(idx, depth);
        (self.blockinfo[idx / 8] >> (idx & 0x7)) & 1 != 0
    }

    fn set_block_in_use(&mut self, idx: usize, depth: usize, used: bool) {
        debugln!(
            "set_block_in_use({:#08x?}, {}, {}) = {}",
            self as *const _,
            idx,
            depth,
            used
        );
        let idx = self.get_idx(idx, depth);
        if used {
            self.blockinfo[idx / 8] |= 1 << (idx & 0x7)
        } else {
            self.blockinfo[idx / 8] &= !(1 << (idx & 0x7))
        }
        debugln!("{:?}", self);
    }

    fn set_block_in_use_recursive(&mut self, idx: usize, depth: usize, used: bool) {
        self.set_block_in_use(idx, depth, used);
        if depth + 1 < NUM_FREELISTS {
            self.set_block_in_use_recursive(idx * 2, depth + 1, used);
            self.set_block_in_use_recursive(idx * 2 + 1, depth + 1, used);
        }
    }

    fn get_freelist_entry(&mut self, idx: usize, depth: usize) -> *mut FreelistEntry {
        let start = (self as *mut _) as usize;
        let offset = (idx << (NUM_FREELISTS - 1 - depth)) * MIN_ALLOC;
        if offset < self.blockinfo.len() {
            core::ptr::null_mut()
        } else {
            (start + offset) as *mut FreelistEntry
        }
    }

    unsafe fn alloc(&mut self, allocator: &mut _HeapAllocator, mut idx: usize, mut depth: usize) {
        debugln!("alloc({:#08x?}, {}, {})", self as *const _, idx, depth);
        self.set_block_in_use_recursive(idx, depth, true);

        // Mark parents as in use
        while depth > 0 && !self.is_block_in_use(idx / 2, depth - 1) {
            // Add the unused sibling to a freelist
            let entry = self.get_freelist_entry(idx ^ 1, depth);
            if !entry.is_null() {
                (*entry).prev = core::ptr::null_mut();
                (*entry).next = allocator.freelists[depth];
                if !allocator.freelists[depth].is_null() {
                    (*allocator.freelists[depth]).prev = entry;
                }
                allocator.freelists[depth] = entry;
            }

            idx /= 2;
            depth -= 1;

            self.set_block_in_use(idx, depth, true);
        }

        // Remove the top-level block from the freelist
        let entry = self.get_freelist_entry(idx, depth);
        if !entry.is_null() {
            let entry = &mut *entry;
            if !entry.prev.is_null() {
                (*entry.prev).next = entry.next;
            } else if allocator.freelists[depth] == entry {
                allocator.freelists[depth] = entry.next;
            }
        }
    }

    unsafe fn free(&mut self, allocator: &mut _HeapAllocator, idx: usize, depth: usize) -> bool {
        if depth == 0 {
            // We just freed the entire page.
            true
        } else if self.is_block_in_use(idx ^ 1, depth) {
            // If the sibling is in use, mark this block (& its children) as free...
            self.set_block_in_use_recursive(idx, depth, false);

            // ...and add the block to the freelist.
            let head: &mut *mut FreelistEntry = &mut allocator.freelists[depth];
            let new: *mut FreelistEntry = self.get_freelist_entry(idx, depth);

            if !new.is_null() {
                (*new).prev = core::ptr::null_mut();
                (*new).next = *head;
                if !head.is_null() {
                    (**head).prev = new;
                }
                *head = new;
            }
            false
        } else {
            debugln!("free parent");
            // If the sibling is free, remove it from the freelist...
            let entry = self.get_freelist_entry(idx ^ 1, depth);
            if !entry.is_null() {
                let entry = &mut *entry;
                if !entry.prev.is_null() {
                    (*entry.prev).next = entry.next;
                } else if allocator.freelists[depth] == entry {
                    allocator.freelists[depth] = entry.next;
                }

                if !entry.next.is_null() {
                    (*entry.next).prev = entry.prev;
                }
            }

            // ...and mark the parent as free instead.
            self.free(allocator, idx / 2, depth - 1)
        }
    }
}

impl core::fmt::Debug for PageHeader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "PageHeader @ {:#08x?} {{", self as *const Self)?;
        for depth in 0..NUM_FREELISTS {
            write!(f, "depth {}: [", depth)?;
            for idx in 0..(1 << depth) {
                write!(
                    f,
                    "{}",
                    if self.is_block_in_use(idx, depth) {
                        "x"
                    } else {
                        " "
                    }
                )?;
            }
            writeln!(f, "]")?;
        }
        write!(f, "}}")?;
        Ok(())
    }
}
