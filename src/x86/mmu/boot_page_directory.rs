use super::*;

/// The initial page directory, loaded at boot by kentry.asm.
#[no_mangle]
pub static BOOT_PAGE_DIRECTORY: pagetables::PageDirectory = generate_boot_page_directory();

/// Runs at compile time to create the boot page directory.
const fn generate_boot_page_directory() -> pagetables::PageDirectory {
    let mut pagedir = [pagetables::Pde::unmapped(); 1024];

    // unfortunately we can't use bitfield functions at compile time,
    // so we have to write out the PDEs (mostly) by hand

    const PDE_SIZE_BITS: usize = 22;
    const FLAGS: usize = 0x2; // writable

    // also, we can't (yet) use for loops at compile time, so we have to use a while loop
    // ¯\_(ツ)_/¯
    let mut virtaddr = KERNEL_RELOC_BASE;
    while virtaddr != 0 {
        let physaddr = virtaddr - KERNEL_RELOC_BASE;

        // Map 0xfxxxxxxx -> 0x0xxxxxxx
        pagedir[(virtaddr >> PDE_SIZE_BITS) as usize] = pagetables::Pde::mapping(
            pagetables::MappingPde::from_bytes((physaddr | FLAGS).to_le_bytes()),
        );

        // Also set up an identity mapping so that we don't crash when activating paging
        pagedir[(physaddr >> PDE_SIZE_BITS) as usize] = pagetables::Pde::mapping(
            pagetables::MappingPde::from_bytes((physaddr | FLAGS).to_le_bytes()),
        );

        virtaddr = virtaddr.wrapping_add(1 << PDE_SIZE_BITS);
    }

    pagetables::PageDirectory(pagedir)
}
