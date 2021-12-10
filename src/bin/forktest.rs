#![no_std]
use ros::{io::File, println, syscall};

fn main() {
    println!("testing fork");

    let (read, write) = syscall::pipe();
    let (mut read, mut write) = (File::new(read), File::new(write));

    match syscall::fork() {
        0 => {
            println!("hello from child!");
            write.close();

            let mut buf = [0u8; 256];
            let len = read.read_all(&mut buf).expect("read error");
            let s = core::str::from_utf8(&buf[0..len]).expect("UTF-8 error");
            println!("read '{}' from parent", s);
            println!("child exiting");
        }

        child => {
            println!("hello from parent of child {}!", child);
            read.close();
            write.write_all(b"Hello, child!").expect("write error");
            println!("parent exiting");
        }
    }
}
