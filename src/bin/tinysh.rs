#![no_std]
use ros::{eprintln, io, print, println, syscall};

const BUFSIZE: usize = 64;

fn main() {
    println!("Welcome to smallersh");
    println!("Usage:");
    println!("    process [ | process ... ] & ...");

    let mut stdin = io::stdin();
    loop {
        print!("> ");

        let mut input = [0u8; BUFSIZE];
        let mut i = 0;
        loop {
            let mut c = 0u8;
            match stdin.read(core::slice::from_mut(&mut c)) {
                Ok(0) => return, // EOF
                Ok(_) => {}
                Err(e) => panic!("input error: {:?}", e),
            };

            input[i] = c;
            match c {
                b'0'..=b'9' | b'|' | b'&' => {}
                b' ' => {}
                b'\n' => {
                    process_command(&input[0..i]);
                    break;
                }
                c => {
                    println!();
                    eprintln!("invalid character: {}", c as char);
                    break;
                }
            }

            i += 1;
            if i == input.len() {
                println!();
                eprintln!("input too long");
                break;
            }
        }
    }
}

fn process_command(command: &[u8]) {
    let iterator = command
        .iter()
        .map(|&c| c as char)
        .filter(|c| !c.is_whitespace())
        .peekable();

    // validate that the command consists of alternating commands & tokens
    {
        let mut iterator = iterator.clone();
        match iterator.peek() {
            None => return, // command is empty
            Some(c) if c.is_digit(10) => {}
            Some(_) => {
                eprintln!("command line must begin with process");
                return;
            }
        }
        while let Some(c) = iterator.next() {
            if let Some(d) = iterator.peek() {
                if c.is_digit(10) && d.is_digit(10) || !c.is_digit(10) && !d.is_digit(10) {
                    eprintln!("parse error");
                    return;
                }
            }
        }
    }

    let mut pipeline_idx = 0;
    let mut pipeline_pids = [0u32; BUFSIZE];
    for c in iterator {
        if let Some(process) = c.to_digit(10) {
            pipeline_pids[pipeline_idx] = process;
            pipeline_idx += 1;
        } else if c == '&' {
            execute_pipeline(&pipeline_pids[0..pipeline_idx], false);
            pipeline_idx = 0;
        }
    }
    execute_pipeline(&pipeline_pids[0..pipeline_idx], true);
}

fn execute_pipeline(processes: &[u32], wait: bool) {
    if processes.is_empty() {
        return;
    }

    let mut child_pids = [0; BUFSIZE];
    let mut child_idx = 0;
    let mut iterator = processes.iter().peekable();
    let mut input = if wait {
        io::stdin().fd
    } else {
        syscall::null_fd()
    };
    while let Some(process) = iterator.next() {
        let (next_input, output) = if iterator.peek().is_some() {
            syscall::pipe()
        } else {
            (input, io::stdout().fd)
        };

        let pid = syscall::fork();
        if pid == 0 {
            // we're the child
            syscall::dup2(input, io::stdin().fd);
            syscall::dup2(output, io::stdout().fd);
            if next_input != input {
                syscall::close(next_input)
            }

            let error = syscall::exec(*process);
            panic!("exec failed: {:?}", error);
        }

        // we're the parent
        child_pids[child_idx] = pid;
        child_idx += 1;
        // Close the child's pipes
        if input > io::stderr().fd {
            syscall::close(input);
        }
        if output > io::stderr().fd {
            syscall::close(output);
        }

        input = next_input;
    }

    if wait {
        for &child in &child_pids[0..child_idx] {
            syscall::wait(child);
        }
    }
}
