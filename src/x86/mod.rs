pub mod interrupt;
pub mod io;
pub mod mmu;

/// Returns the current code segment.
pub fn cs() -> u16 {
    unsafe {
        let result: u16;
        asm!("mov {:x}, cs", out(reg) result, options(nomem, nostack));
        result
    }
}
