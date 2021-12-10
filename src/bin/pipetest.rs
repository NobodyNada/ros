#![no_std]
use ros::{io::File, syscall};

fn main() {
    let (read, write) = syscall::pipe();
    let (mut read, mut write) = (File::new(read), File::new(write));

    const TEST_STR: &str = "Hello, world!";

    write.write_all(TEST_STR.as_bytes()).expect("write error");

    let mut buf = [0u8; TEST_STR.len()];
    assert_eq!(read.read_all(&mut buf).expect("read error"), buf.len());
    assert_eq!(buf, TEST_STR.as_bytes());

    write.close();
    assert_eq!(read.read_all(&mut buf).expect("read error"), 0);

    ros::println!("pipetest passed");
}
