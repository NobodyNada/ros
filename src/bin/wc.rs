#![no_std]
use ros::{io, println};

fn main() {
    let mut stdin = io::stdin();

    let mut chars: usize = 0;
    let mut words: usize = 0;
    let mut lines: usize = 0;
    let mut in_word = false;
    loop {
        let mut c = 0u8;
        match stdin.read(core::slice::from_mut(&mut c)) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => panic!("input error: {:?}", e),
        };

        chars += 1;
        if c == b'\n' {
            lines += 1;
        }
        if (c as char).is_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            words += 1;
        }
    }

    println!("{} {} {}", lines, words, chars);
}
