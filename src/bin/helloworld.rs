#![no_std]
use ros::syscall;

fn main() {
    syscall::puts("Hello, world!");
}
