pub mod boot_page_directory;
pub mod mmap;
pub mod pagetables;

mod palloc;
pub use palloc::{palloc, palloc_zeroed};

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

/// Sets up virtual & physical memory management.
pub fn init_mmu() {
    // Point the GDT to a virtual address rather than a physical address.
    let mut gdtr = crate::x86::DescriptorTableRegister::sgdt();
    gdtr.paddr |= KERNEL_RELOC_BASE;
    gdtr.lgdt();

    // Initialize memory mappings
    mmap::MAPPER.take().unwrap().init()
}
