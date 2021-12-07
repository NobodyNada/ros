#![no_std]
#![feature(asm)]
use ros::syscall;

#[allow(deref_nullptr)]
fn main() {
    syscall::puts("testing a user pagefault");
    unsafe {
        asm!("mov dword ptr [{}], 0x1234", in(reg) 0);
    }
}
