//! ROS virtual memory management
//!
//! Address 0x0-0x0fffffff are reserved for userspace, while 0xf0000000+ are kernelspace.
//! The page directory and pagetables for the current process are mapped at the very end of the
//! virtual address
//!
//! # Virtual address space diagram
//!
//! ```none
//! +-------------------------------+ 0xffffffff____
//! | Pagetables for active process |               \__
//! | (in order, covering the whole |                  \__
//! |    virtual address space)     |                     \
//! +-------------------------------+ 0xffc01000___        +----------------+ 0x400000
//! |      Page directory for       |              \       |   PTs for PTs  |
//! |       active process          |               \      |  (PER-PROCESS) |
//! +-------------------------------+ 0xffc00000___  \     +----------------+ 0x3ff000
//! |      Physical page info       |              \  \    |   Kernel PTs   |          
//! |        (see palloc.rs)        |               \  \   |    (SHARED)    |          
//! +-------------------------------+ 0xff800000     \  \  +----------------+ 0x3c0000
//! |              ...              |                 \  \ |  Userspace PTs |          
//! +-------------------------------+                  \  \|  (PER-PROCESS) |          
//! |         Kernel heap           |                   \  +----------------+ 0x001000  
//! +-------------------------------+                    \ | Page directory |           
//! |      Kernel stack/BSS         |                     \|  (PER-PROCESS) |           
//! +-------------------------------+                      +----------------+ 0x000000  
//! |        Kernel RODATA          |                   
//! +-------------------------------+                   
//! |        Kernel RODATA          |                 
//! +-------------------------------+              
//! |         Kernel TEXT           |              
//! +-------------------------------+ 0xf0000000
//! |                               |
//! |                               |
//! |                               |
//! |       Userspace (TBD)         |
//! |                               |
//! |                               |
//! +-------------------------------+ 0x00400000
//! |            NULL               |
//! |  (unmapped in page directory) |
//! +-------------------------------+ 0x00000000
//! ```
//!
//! (Not to scale)

#![allow(dead_code)]

use core::mem::MaybeUninit;

use crate::x86::mmu::{self, pagetables};
use mmu::palloc::PhysAllocator;
use modular_bitfield::prelude::*;

#[bitfield]
#[derive(Clone, Copy)]
pub struct MappingFlags {
    pub writable: bool,
    pub user_accessible: bool,

    #[skip]
    __: B6,
}

pub(super) const PAGEINFO_BASE: usize = 0xff800000;
pub(super) const PAGETABLE_BASE: usize = 0xffc00000;

// A physical page which is always zeroed.
static ZERO_PAGE: mmu::PageAligned<[u8; mmu::PAGE_SIZE]> = mmu::PageAligned([0; mmu::PAGE_SIZE]);
fn zero_page_paddr() -> usize {
    core::ptr::addr_of!(ZERO_PAGE) as usize & !(mmu::KERNEL_RELOC_BASE as usize)
}

#[derive(Default)]
pub struct MemoryMapper;

impl MemoryMapper {
    /// Zero-initializes 'count' virtual pages (using copy-on-write semantics). Note that the 'writable' field of
    /// the mapping flags is ignored.
    pub fn map_zeroed(
        &mut self,
        palloc: &mut PhysAllocator,
        mut vaddr: usize,
        mut count: usize,
        flags: MappingFlags,
    ) {
        loop {
            self.map(palloc, zero_page_paddr(), vaddr, flags.with_writable(false));

            count -= 1;
            if count == 0 {
                break;
            } else {
                vaddr = vaddr
                    .checked_add(mmu::PAGE_SIZE)
                    .expect("virtual address out of range");
                continue;
            }
        }
    }

    pub fn map(
        &mut self,
        palloc: &mut PhysAllocator,
        paddr: usize,
        vaddr: usize,
        flags: MappingFlags,
    ) {
        self.map_ensure_pagetable(palloc, vaddr);
        unsafe {
            self.map_no_alloc(paddr, vaddr, flags);
        }
    }

    pub fn unmap(&mut self, palloc: &mut PhysAllocator, vaddr: usize) {
        self.map_ensure_pagetable(palloc, vaddr);
        unsafe {
            *(self.get_pte_ptr(vaddr) as *mut pagetables::Pte) = pagetables::Pte::unmapped();
        }
    }

    /// Finds and returns a block of 'pages' unmapped pages in the kernel portion of the virtual address space.
    pub fn find_unused_kernelspace(&self, pages: usize) -> Option<usize> {
        let mut vaddr = mmu::KERNEL_RELOC_BASE as usize;
        let mut base = vaddr;
        let mut contiguous: usize = 0;

        loop {
            if self.get_mapping(vaddr).is_none() {
                contiguous += 1;
                if contiguous == pages {
                    return Some(base);
                }
                vaddr = vaddr.checked_add(mmu::PAGE_SIZE)?;
            } else {
                contiguous = 0;
                vaddr = vaddr.checked_add(mmu::PAGE_SIZE)?;
                base = vaddr;
            }
        }
    }

    /// Finds and returns a block of 'pages' unmapped pages in the user portion of the virtual address space.
    pub fn find_unused_userspace(&self, pages: usize) -> Option<usize> {
        let mut vaddr = 1024 * mmu::PAGE_SIZE as usize; // skip the null page
        let mut base = vaddr;
        let mut contiguous: usize = 0;

        while vaddr < mmu::KERNEL_RELOC_BASE as usize {
            if self.get_mapping(vaddr).is_none() {
                contiguous += 1;
                if contiguous == pages {
                    return Some(base);
                }
                vaddr += mmu::PAGE_SIZE;
            } else {
                contiguous = 0;
                vaddr += mmu::PAGE_SIZE;
                base = vaddr;
            }
        }

        // if we ran into kernelspace, we didn't find a contiguous block big enough
        None
    }

    /// Ensures the pagetable for `vaddr` is allocated, listed in the page directory,
    /// and not marked as copy-on-write.
    fn map_ensure_pagetable(&mut self, palloc: &mut PhysAllocator, vaddr: usize) {
        let ptaddr = self.get_pte_ptr(vaddr);
        if self.get_mapping(ptaddr).is_none() {
            // If the PTE is unmapped, map it.
            let ptpaddr = palloc.alloc().expect("out of memory");
            self.map(
                palloc,
                ptpaddr,
                ptaddr,
                MappingFlags::new().with_writable(true),
            );

            // Zero the pagetable.
            unsafe {
                let pagetable = ptaddr as *mut u8;
                pagetable.write_bytes(0, mmu::PAGE_SIZE);
            }
        } else {
            // The pagetable is already mapped, but it may be copy-on-write.
            self.cow_if_needed(palloc, ptaddr);
        }

        let page_directory = PAGETABLE_BASE as *mut pagetables::PageDirectory;
        let pde_index = vaddr >> 22;
        let pde = unsafe { &mut (*page_directory).0[pde_index] };
        let physptaddr = self.get_mapping(ptaddr).unwrap().physaddr() as usize;

        if pde.get_pagetable().map(|m| m.ptaddr() as usize) != Some(physptaddr) {
            *pde = pagetables::Pde::pagetable(
                pagetables::PagetablePde::new()
                    .with_ptaddr(physptaddr as u32)
                    .with_is_writable(true)
                    .with_userspace_accessible(true),
            );
        }
    }

    /// Like 'map', but guaranteed not to allocate any memory to store the pagetable.
    /// The caller is required to ensure the pagetable is already allocated by calling
    /// `map_ensure_pagetable`.
    unsafe fn map_no_alloc(&mut self, paddr: usize, vaddr: usize, flags: MappingFlags) {
        *(self.get_pte_ptr(vaddr) as *mut pagetables::Pte) = pagetables::Pte::mapping(
            pagetables::MappingPte::new()
                .with_physaddr(paddr as u32)
                .with_is_writable(flags.writable())
                .with_userspace_accessible(flags.user_accessible()),
        );

        // Flush the TLB for vaddr
        asm!("invlpg [{}]", in(reg) vaddr, options(nomem, nostack));
    }

    fn _get_pte_ptr(vaddr: usize) -> usize {
        PAGETABLE_BASE | ((vaddr >> mmu::PAGE_SHIFT) * core::mem::size_of::<pagetables::Pte>())
    }

    /// Returns the virtual address which would point to the pagetable entry mapping `vaddr`.
    /// Note that the pagetable may not actually exist; call `get_mapping` on the returned pointer
    /// to find out.
    pub fn get_pte_ptr(&self, vaddr: usize) -> usize {
        MemoryMapper::_get_pte_ptr(vaddr)
    }

    /// Returns the pagetable entry mapping `vaddr`, if it is present.
    pub fn get_mapping(&self, vaddr: usize) -> Option<pagetables::MappingPte> {
        // We don't know whether or not 'vaddr' is mapped; we also don't know whether the
        // pagetable containing the mapping for 'vaddr' is mapped. However, the pagetable mapping
        // the pagetables fits into a single page, and is always mapped, so we don't need to check
        // in that case.
        if vaddr < self.get_pte_ptr(PAGETABLE_BASE)
            && self.get_mapping(self.get_pte_ptr(vaddr)).is_none()
        {
            return None;
        }

        unsafe { (*(self.get_pte_ptr(vaddr) as *const pagetables::Pte)).get() }
    }

    pub fn mapping_to_flags(&self, mapping: pagetables::MappingPte) -> MappingFlags {
        MappingFlags::new()
            .with_writable(mapping.is_writable())
            .with_user_accessible(mapping.userspace_accessible())
    }

    pub fn get_mapping_flags(&self, vaddr: usize) -> Option<MappingFlags> {
        self.get_mapping(vaddr).map(|m| self.mapping_to_flags(m))
    }

    /// If copy-on-write is enabled for 'vaddr', copies to a new page (in order to ensure 'vaddr'
    /// is writable). Returns true if a page was copied, or false if no copy was needed (or the
    /// page was not marked copy-on-write).
    ///
    /// Panics if the page is not mapped.
    pub fn cow_if_needed(&mut self, palloc: &mut PhysAllocator, vaddr: usize) -> bool {
        let vaddr = mmu::page_align_down(vaddr);
        let mapping = self.get_mapping(vaddr).expect("vaddr is unmapped");
        let src_paddr = mapping.physaddr() as usize;

        // Special case: the zero page is always copy-on-write
        let zero_page_paddr =
            core::ptr::addr_of!(ZERO_PAGE) as usize & !(mmu::KERNEL_RELOC_BASE as usize);
        let is_zero_page = src_paddr == zero_page_paddr;

        unsafe {
            let info = (*palloc.get_page_info(src_paddr)).allocated;
            if is_zero_page || (info.copy_on_write() && info.refcount() > 0) {
                // We need to copy the page to a temporary buffer, map a new page, and copy the
                // data into the new page. However, we need to be a little careful: we store the
                // temporary buffer on the stack, so we want to avoid recursive calls to this
                // function while the temporary buffer is alive, in order to avoid using a ton of
                // stack space. But the physical memory allocator can trigger copy-on-writes, so we
                // must ensure all allocations happen before creating a temporary buffer.

                // First, allocate a new physical page to store the result.
                let dest_paddr = palloc.alloc().expect("not enough memory for copy-on-write");

                // Also preallocate the pagetable if necessary as the pagetable could itself be COW
                self.map_ensure_pagetable(palloc, vaddr);

                // Copy the old page to a temporary buffer, map the new page and copy the contents
                // into the new page.
                //
                // NOTE: Do this in a separate stack frame.
                #[inline(never)]
                unsafe fn do_copy(
                    mapper: &mut MemoryMapper,
                    vaddr: usize,
                    dest_paddr: usize,
                    flags: MappingFlags,
                ) {
                    let mut contents: [MaybeUninit<u8>; mmu::PAGE_SIZE] =
                        MaybeUninit::uninit().assume_init();
                    let contents_ptr = contents.as_mut_ptr() as *mut u8;
                    core::ptr::copy(vaddr as *const u8, contents_ptr, mmu::PAGE_SIZE);

                    // Map a new physical page
                    mapper.map_no_alloc(dest_paddr, vaddr, flags);

                    // Copy the contents of the old page to the new page
                    core::ptr::copy(contents_ptr, vaddr as *mut u8, mmu::PAGE_SIZE);
                }

                do_copy(
                    self,
                    vaddr,
                    dest_paddr,
                    self.mapping_to_flags(mapping).with_writable(true),
                );

                // Release our reference to the old page.
                if !is_zero_page {
                    palloc.free(src_paddr, self);
                }

                true
            } else if info.copy_on_write() && info.refcount() == 0 {
                // The page was copy-on-write, but there is now only one reference to it.
                // We can just go ahead and mark it as owned.

                *palloc.get_page_info_mut(src_paddr, self) = mmu::palloc::PhysPageInfo {
                    allocated: Default::default(),
                };
                self.map(
                    palloc,
                    src_paddr,
                    vaddr,
                    self.mapping_to_flags(mapping).with_writable(true),
                );

                true
            } else {
                false
            }
        }
    }

    pub(super) unsafe fn init(&mut self, palloc: &mut PhysAllocator) {
        let page_directory =
            palloc.alloc().expect("out of memory") as *mut pagetables::PageDirectory;
        core::ptr::write_bytes(page_directory as *mut u8, 0, mmu::PAGE_SIZE);

        unsafe fn map(
            page_directory: *mut pagetables::PageDirectory,
            palloc: &mut PhysAllocator,
            paddr: usize,
            vaddr: usize,
            writable: bool,
        ) {
            let pde_index = vaddr >> 22;
            assert!(pde_index > 0, "cannot map null page");
            let pde = &mut (*page_directory).0[pde_index];

            let (pt, is_new) = match pde.get_pagetable() {
                Some(pt) => (pt.ptaddr() as usize as *mut pagetables::Pagetable, false),
                None => {
                    let pt = palloc.alloc().expect("out of memory");
                    core::ptr::write_bytes(pt as *mut u8, 0, mmu::PAGE_SIZE);
                    *pde = pagetables::Pde::pagetable(
                        pagetables::PagetablePde::new()
                            .with_ptaddr(pt as u32)
                            .with_is_writable(true)
                            .with_userspace_accessible(true),
                    );
                    (pt as usize as *mut pagetables::Pagetable, true)
                }
            };

            let pte = &mut (*pt).0[(vaddr >> 12) & 0x3ff];
            assert!(!pte.is_present(), "duplicate mapping");
            *pte = pagetables::Pte::mapping(
                pagetables::MappingPte::new()
                    .with_physaddr(paddr as u32)
                    .with_is_writable(writable),
            );

            // If we created a new pagetable, map it too!
            if is_new {
                map(
                    page_directory,
                    palloc,
                    pt as usize,
                    mmu::page_align_down(MemoryMapper::_get_pte_ptr(vaddr)),
                    true,
                );
            }
        }

        let map_rw = |palloc: &mut _, paddr, vaddr| map(page_directory, palloc, paddr, vaddr, true);
        let map_ro =
            |palloc: &mut _, paddr, vaddr| map(page_directory, palloc, paddr, vaddr, false);

        // Map the page directory itself
        map_rw(palloc, page_directory as usize, PAGETABLE_BASE);

        // Map the kernel's identity mappings
        let mut paddr = 0;
        while paddr <= palloc.get_max_allocated() {
            map_rw(palloc, paddr, paddr + mmu::KERNEL_RELOC_BASE as usize);
            paddr += mmu::PAGE_SIZE;
        }

        // Map the physical memory map, as read-only zeroes (for copy-on-write)
        let zero = core::ptr::addr_of!(ZERO_PAGE) as usize - mmu::KERNEL_RELOC_BASE as usize;
        let mut vaddr = PAGEINFO_BASE;
        while vaddr < PAGETABLE_BASE {
            map_ro(palloc, zero, vaddr);
            vaddr += mmu::PAGE_SIZE;
        }

        asm!("mov cr3, {}", in(reg) page_directory);
    }
}

impl core::fmt::Debug for MemoryMapper {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut f = f.debug_map();

        let mut printed_prev = true;
        for vaddr in (0..=usize::MAX).step_by(mmu::PAGE_SIZE) {
            if let Some(mapping) = self.get_mapping(vaddr) {
                f.entry(&vaddr, &mapping.physaddr());
                printed_prev = true;
            } else if printed_prev {
                f.entry(&"...", &"unmapped");
                printed_prev = false;
            }
        }

        f.finish()
    }
}
