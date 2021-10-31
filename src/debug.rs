pub fn backtrace<F>(mut handler: F)
where
    F: FnMut(usize),
{
    unsafe {
        let mut ebp: *const usize;
        asm!("mov {}, ebp", lateout(reg) ebp, options(nomem, nostack));
        while !ebp.is_null() {
            let pc = *ebp.offset(1);
            handler(pc);
            ebp = *ebp as *const _;
        }
    }
}
