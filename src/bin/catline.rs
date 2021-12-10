#![no_std]
use ros::io;

fn main() {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();
    loop {
        let mut c = 0u8;
        match stdin.read(core::slice::from_mut(&mut c)) {
            Ok(0) => return, // EOF
            Ok(_) => {}
            Err(e) => panic!("input error: {:?}", e),
        };

        let bytes_written = stdout
            .write(core::slice::from_ref(&c))
            .expect("output error");
        if bytes_written == 0 {
            return; // EOF
        }

        if c == b'\n' {
            return; // EOL
        }
    }
}
