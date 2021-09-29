pub mod boot_page_directory;
pub mod pagetables;

// NOTE: duplicated in kentry.asm
pub const KERNEL_RELOC_BASE: u32 = 0xf0000000;
