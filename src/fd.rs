use crate::{
    syscall::{ReadError, WriteError},
    x86::io,
};

/// A file descriptor backend.
pub trait File {
    /// Attempts to read from the file descriptor. Returns the number of bytes read, or an error.
    /// The default implementation always returns ReadError::Unsupported.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ReadError> {
        let _ = buf;
        Err(ReadError::Unsupported)
    }
    /// Returns true if we have bytes available for reading.
    /// The default implementation always returns true
    fn can_read(&self) -> bool {
        true
    }

    /// Attempts to write to the file descriptor. Returns the number of bytes written, or an error.
    fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        let _ = buf;
        Err(WriteError::Unsupported)
    }
    /// Returns true if we have space available for writing.
    /// The default implementation always returns true
    fn can_write(&self) -> bool {
        true
    }
}

pub struct Console;
impl File for Console {
    fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        io::serial::COM1
            .take()
            .expect("serial conflict")
            .write_bytes(buf);
        io::cga::CGA.take().expect("CGA conflict").write_bytes(buf);

        Ok(buf.len())
    }
}
