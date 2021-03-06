use crate::syscall;
use core::fmt::{self, Write};

pub type Fd = u32;

/// A wrapper around a raw file descriptor.
#[derive(Clone)]
pub struct File {
    pub fd: Fd,
}
impl File {
    pub fn new(fd: Fd) -> File {
        File { fd }
    }
}

/// Returns a file referencing the standard input stream.
pub fn stdin() -> File {
    File { fd: 0 }
}
/// Returns a file referencing the standard output stream.
pub fn stdout() -> File {
    File { fd: 1 }
}
/// Returns a file referencing the standard error stream.
pub fn stderr() -> File {
    File { fd: 2 }
}

impl File {
    /// Reads up to 'buf.len()' bytes from the file, returning the number of bytes read.
    /// Blocks if no data is available. Returns 0 if the end-of-file is reached.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, syscall::ReadError> {
        syscall::read(self.fd, buf)
    }

    /// Attemts to read from the file until `buf` is full.
    /// Note that the buffer still will not be filled if the end-of-file is reached.
    pub fn read_all(&mut self, buf: &mut [u8]) -> Result<usize, syscall::ReadError> {
        let mut i = 0;
        while i < buf.len() {
            match syscall::read(self.fd, &mut buf[i..])? {
                0 => break,
                n => i += n,
            }
        }
        Ok(i)
    }

    /// Writes up to 'buf.len()' bytes to the file, returning the number of bytes written.
    /// Blocks if no space is available.
    pub fn write(&mut self, buf: &[u8]) -> Result<usize, syscall::WriteError> {
        syscall::write(self.fd, buf)
    }

    /// Writes the entire buffer to the file.
    pub fn write_all(&mut self, buf: &[u8]) -> Result<(), syscall::WriteError> {
        let mut i = 0;
        while i < buf.len() {
            i += syscall::write(self.fd, &buf[i..])?;
        }
        Ok(())
    }

    /// Closes the file.
    pub fn close(self) {
        syscall::close(self.fd)
    }
}

impl Write for File {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let mut buf = s.as_bytes();
        while !buf.is_empty() {
            let bytes_written = self.write(buf).map_err(|_| fmt::Error)?;
            buf = &buf[bytes_written..];
        }
        Ok(())
    }
}

#[doc(hidden)]
pub fn _fprint(file: &mut File, format: fmt::Arguments) -> fmt::Result {
    file.write_fmt(format)
}

#[macro_export]
macro_rules! fprint {
    ($file:expr, $($arg:tt)*) => (
        $crate::io::_fprint($file, core::format_args!($($arg)*))
    )
}
#[macro_export]
macro_rules! fprintln {
    ($file: expr, $($arg:tt)*) => (
        $crate::fprint!($file, "{}\n", core::format_args!($($arg)*))
    )
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => (
        $crate::fprint!(&mut $crate::io::stdout(), $($arg)*)
            .expect("I/O error")
    )
}
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => (
        $crate::fprintln!(&mut $crate::io::stdout(), $($arg)*)
            .expect("I/O error")
    )
}

#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => (
        $crate::fprint!(&mut $crate::io::stderr(), $($arg)*)
            .expect("I/O error")
    )
}
#[macro_export]
macro_rules! eprintln {
    () => ($crate::eprint!("\n"));
    ($($arg:tt)*) => (
        $crate::fprintln!(&mut $crate::io::stderr(), $($arg)*)
            .expect("I/O error")
    )
}
