#![no_std]
use ros::syscall;

fn main() {
    for s in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"] {
        syscall::puts(s);
        syscall::yield_cpu();
    }
}
