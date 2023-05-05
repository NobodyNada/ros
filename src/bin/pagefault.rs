#![no_std]
use core::arch::asm;

#[allow(deref_nullptr)]
fn main() {
    ros::println!("testing a user pagefault");
    unsafe {
        asm!("mov dword ptr [{}], 0x1234", in(reg) 0);
    }
}
