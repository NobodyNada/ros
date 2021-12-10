#![no_std]

fn main() {
    loop {
        ros::syscall::yield_cpu();
    }
}
