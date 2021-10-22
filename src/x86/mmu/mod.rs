use crate::util::Global;

pub mod boot_page_directory;
pub mod mmap;
pub mod pagetables;
pub mod palloc;

// NOTE: duplicated in kentry.asm
pub const KERNEL_RELOC_BASE: u32 = 0xf0000000;

extern "C" {
    pub static KERNEL_VIRT_START: u8;
}

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
pub const PAGE_MASK: usize = !(PAGE_SIZE - 1);

#[repr(align(4096))]
pub struct PageAligned<T>(pub T);

/// Aligns a memory address by rounding it down to the start of the page.
pub const fn page_align_down(addr: usize) -> usize {
    addr & PAGE_MASK
}

/// Aligns a memory address by rounding it up to the next page.
/// Returns None of the operation overflows.
pub const fn page_align_up(addr: usize) -> Option<usize> {
    if let Some(a) = addr.checked_add(!PAGE_MASK) {
        Some(a & PAGE_MASK)
    } else {
        None
    }
}

pub struct Mmu {
    initialized: bool,
    allocator: palloc::PhysAllocator,
    mapper: mmap::MemoryMapper,
}
pub static MMU: Global<Mmu> = Global::lazy(|| unsafe { Mmu::new() });

impl Mmu {
    unsafe fn new() -> Self {
        Self {
            initialized: false,
            allocator: palloc::PhysAllocator::new(),
            mapper: Default::default(),
        }
    }

    pub fn init(&mut self) {
        assert!(!self.initialized, "attempt to initialize MMU twice");

        // Point the GDT to a virtual address
        // (rather than a physical address as set up by the bootloader).
        let mut gdtr = crate::x86::DescriptorTableRegister::sgdt();
        gdtr.paddr |= KERNEL_RELOC_BASE;
        gdtr.lgdt();

        unsafe {
            self.mapper.init(&mut self.allocator);
        }
    }

    pub fn get_allocator(&mut self) -> &mut palloc::PhysAllocator {
        &mut self.allocator
    }
    pub fn get_mapper(&mut self) -> &mut mmap::MemoryMapper {
        &mut self.mapper
    }

    /// Attempts to allocate a pages of memory. Returns the address of the allocated page, or None
    /// if no more memory is available.
    pub fn palloc(&mut self) -> Option<usize> {
        self.allocator.alloc()
    }

    pub fn map(&mut self, vaddr: usize, paddr: usize, flags: mmap::MappingFlags) {
        assert!(self.initialized);

        self.mapper.map(&mut self.allocator, paddr, vaddr, flags)
    }
}
