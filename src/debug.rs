use crate::mmu::pagefault;
use core::arch::asm;

pub fn backtrace<F>(mut handler: F) -> Option<pagefault::PageFaultCode>
where
    F: FnMut(usize),
{
    unsafe {
        let mut ebp: *const usize;
        asm!("mov {}, ebp", lateout(reg) ebp, options(nomem, nostack));
        while !ebp.is_null() {
            let pc = match pagefault::try_read(ebp.offset(1)) {
                Ok(pc) => pc,
                Err(code) => return Some(code),
            };
            handler(pc);
            ebp = match pagefault::try_read(ebp) {
                Ok(ebp) => ebp as *const _,
                Err(code) => return Some(code),
            };
        }
    }

    None
}
