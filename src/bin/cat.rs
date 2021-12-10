#![no_std]

fn main() {
    let mut buf = [0u8; 256];
    let mut stdin = ros::io::stdin();
    let mut stdout = ros::io::stdout();
    loop {
        let bytes_read = stdin.read(&mut buf).expect("read error");
        if bytes_read == 0 {
            // EOF
            return;
        }

        let mut i = 0;
        while i < bytes_read {
            let bytes_written = stdout.write(&buf[i..bytes_read]).expect("write error");
            if bytes_written == 0 {
                // EOF
                return;
            }
            i += bytes_written;
        }
    }
}
