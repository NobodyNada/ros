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

use crate::util::Global;
use crate::x86::mmu::{self, pagetables};
use modular_bitfield::prelude::*;

#[bitfield]
pub struct MappingFlags {
    writable: bool,
    executable: bool,
    user_accessible: bool,

    #[skip]
    __: B5,
}

const PAGEINFO_BASE: usize = 0xff800000;
const PAGETABLE_BASE: usize = 0xffc00000;

// A physical page which is always zeroed.
static ZERO_PAGE: mmu::PageAligned<[u8; mmu::PAGE_SIZE]> = mmu::PageAligned([0; mmu::PAGE_SIZE]);

#[derive(Default)]
pub struct MemoryMapper {
    initialized: bool,
}
pub static MAPPER: Global<MemoryMapper> = Global::lazy_default();

impl MemoryMapper {
    pub fn map_zeroed(&mut self, _vaddr: usize, _pages: usize, _flags: MappingFlags) {
        todo!()
    }

    pub fn map(&mut self, _paddr: usize, _vaddr: usize, _count: usize, _flags: MappingFlags) {
        todo!()
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

    pub fn init(&mut self) {
        assert!(!self.initialized, "attempt to initialize MMU twice");
        self.initialized = true;

        let page_directory =
            mmu::palloc_zeroed().expect("out of memory") as *mut pagetables::PageDirectory;

        unsafe fn map(
            page_directory: *mut pagetables::PageDirectory,
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
                    let pt = mmu::palloc_zeroed().expect("out of memory");
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
                    pt as usize,
                    mmu::page_align_down(MemoryMapper::_get_pte_ptr(vaddr)),
                    true,
                );
            }
        }

        unsafe {
            let map_rw = |paddr, vaddr| map(page_directory, paddr, vaddr, true);
            let map_ro = |paddr, vaddr| map(page_directory, paddr, vaddr, false);

            // Map the page directory itself
            map_rw(page_directory as usize, PAGETABLE_BASE);

            // Map the kernel's identity mappings
            let mut paddr = 0;
            while paddr <= mmu::palloc::ALLOCATOR.take().unwrap().get_max_allocated() {
                map_rw(paddr, paddr + mmu::KERNEL_RELOC_BASE as usize);
                paddr += mmu::PAGE_SIZE;
            }

            // Map the physical memory map, as read-only zeroes (for copy-on-write)
            let zero = core::ptr::addr_of!(ZERO_PAGE) as usize - mmu::KERNEL_RELOC_BASE as usize;
            let mut vaddr = PAGEINFO_BASE;
            while vaddr < PAGETABLE_BASE {
                map_ro(zero, vaddr);
                vaddr += mmu::PAGE_SIZE;
            }

            asm!("mov cr3, {}", in(reg) page_directory);
        }
    }
}

impl core::fmt::Debug for MemoryMapper {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if !self.initialized {
            return f.write_str("<memory mappings not initialized>");
        }

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
