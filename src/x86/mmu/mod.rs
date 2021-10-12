pub mod boot_page_directory;
pub mod pagetables;

mod palloc;
pub use palloc::palloc;

// NOTE: duplicated in kentry.asm
pub const KERNEL_RELOC_BASE: u32 = 0xf0000000;

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
pub const PAGE_MASK: usize = !(PAGE_SIZE - 1);

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
