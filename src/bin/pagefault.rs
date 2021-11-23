#![no_std]
#![feature(asm)]

#[allow(deref_nullptr)]
fn main() {
    ros::puts("testing a user pagefault");
    unsafe {
        asm!("mov dword ptr [{}], 0x1234", in(reg) 0);
    }
}
