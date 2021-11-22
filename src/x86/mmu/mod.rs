use crate::util::Global;

pub mod boot_page_directory;
pub mod mmap;
pub mod pagefault;
pub mod pagetables;
pub mod palloc;
pub mod segment;

// NOTE: duplicated in kentry.asm
pub const KERNEL_RELOC_BASE: usize = 0xf0000000;

extern "C" {
    pub static KERNEL_VIRT_START: u8;
    pub static KERNEL_VIRT_END: u8;
}

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
pub const PAGE_MASK: usize = !(PAGE_SIZE - 1);

pub const NUM_SEGMENTS: usize = 6;

#[repr(u16)]
pub enum SegmentId {
    Null = 0,
    KernelCode = 0x8,
    KernelData = 0x10,
    UserCode = 0x18 | 3,
    UserData = 0x20 | 3,
    TaskState = 0x28,
}

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
    pub allocator: palloc::PhysAllocator,
    pub mapper: mmap::MemoryMapper,
    pub gdt: [segment::SegmentDescriptor; NUM_SEGMENTS],
}
pub static MMU: Global<Mmu> = Global::lazy(|| unsafe { Mmu::new() });

impl Mmu {
    unsafe fn new() -> Self {
        Self {
            initialized: false,
            allocator: palloc::PhysAllocator::new(),
            mapper: Default::default(),
            gdt: [
                // Null
                segment::SegmentDescriptor::new(),
                // Kernel code
                segment::SegmentDescriptor::new()
                    .with_present(true)
                    .with_base(0)
                    .with_limit(0xfffff)
                    .with_page_granularity(true)
                    .with_segment_type(0b1010)
                    .with_code_or_data(true)
                    .with_dpl(0)
                    .with_db(true),
                // Kernel data
                segment::SegmentDescriptor::new()
                    .with_present(true)
                    .with_base(0)
                    .with_limit(0xfffff)
                    .with_page_granularity(true)
                    .with_segment_type(0b0010)
                    .with_code_or_data(true)
                    .with_dpl(0)
                    .with_db(true),
                // User code
                segment::SegmentDescriptor::new()
                    .with_present(true)
                    .with_base(0)
                    .with_limit(0xfffff)
                    .with_page_granularity(true)
                    .with_segment_type(0b1010)
                    .with_code_or_data(true)
                    .with_dpl(3)
                    .with_db(true),
                // User data
                segment::SegmentDescriptor::new()
                    .with_present(true)
                    .with_base(0)
                    .with_limit(0xfffff)
                    .with_page_granularity(true)
                    .with_segment_type(0b0010)
                    .with_code_or_data(true)
                    .with_dpl(3)
                    .with_db(true),
                // Task state segment (initialized later)
                segment::SegmentDescriptor::new(),
            ],
        }
    }

    pub fn init(&mut self) {
        assert!(!self.initialized, "attempt to initialize MMU twice");

        // Load the kernel GDT
        let gdtr = crate::x86::DescriptorTableRegister::new(
            (core::mem::size_of_val(&self.gdt) - 1) as u16,
            core::ptr::addr_of!(self.gdt) as usize,
        );
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
