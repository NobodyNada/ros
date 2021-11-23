#![no_std]

fn main() {
    for s in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"] {
        ros::puts(s);
        ros::yield_cpu();
    }
}
