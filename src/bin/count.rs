#![no_std]
use ros::syscall;

fn main() {
    for i in 0..10 {
        ros::println!("{}", i);
        syscall::yield_cpu();
    }
}
