pub mod io;
pub mod mmu;

/// Disables interrupts.
pub fn cli() {
    unsafe {
        asm!("cli");
    }
}

/// Enables interrupts.
pub fn sei() {
    unsafe {
        asm!("sei");
    }
}
