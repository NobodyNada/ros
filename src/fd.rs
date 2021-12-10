use alloc::alloc;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

use crate::{
    syscall::{ReadError, WriteError},
    x86::io,
};

/// A file descriptor backend.
pub trait File {
    /// Attempts to read from the file descriptor. Returns the number of bytes read, or an error.
    /// A return value of 0 indicates end-of-file.
    /// The default implementation always returns ReadError::Unsupported.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ReadError> {
        let _ = buf;
        Err(ReadError::Unsupported)
    }
    /// Returns true if we can read without blocking.
    /// The default implementation always returns true, because we can return an "unsupported"
    /// error without blocking.
    fn can_read(&mut self) -> bool {
        true
    }

    /// Attempts to write to the file descriptor. Returns the number of bytes written, or an error.
    /// A return value of 0 indicates end-of-file.
    fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        let _ = buf;
        Err(WriteError::Unsupported)
    }
    /// Returns true if we can write without blocking.
    /// The default implementation always returns true, because we can return an "unsupported"
    /// error without blocking.
    fn can_write(&mut self) -> bool {
        true
    }

    /// Returns true if this file descriptor can be accessed in the given manner (read or write).
    fn can_access(&mut self, ty: AccessType) -> bool {
        match ty {
            AccessType::Read => self.can_read(),
            AccessType::Write => self.can_write(),
        }
    }
}

#[derive(Clone, Copy)]
pub enum AccessType {
    Read,
    Write,
}

const CONSOLE_BUFSIZE: usize = 4096;
pub struct ConsoleBuffer {
    buf: AtomicPtr<u8>,
    rpos: AtomicUsize,
    epos: AtomicUsize,
    wpos: AtomicUsize,

    read_lock: AtomicBool,
    write_lock: AtomicBool,
}
pub static CONSOLE_BUFFER: ConsoleBuffer = ConsoleBuffer::new();

impl ConsoleBuffer {
    pub fn handle_echo(&self) {
        assert!(
            !self.read_lock.swap(true, Ordering::Acquire),
            "simultaneous read from console buffer"
        );
        unsafe {
            self._handle_echo();
        }
        self.read_lock.store(false, Ordering::Release);
    }

    /// Initializes the console buffer.
    ///
    /// # Safety
    ///
    /// The caller must ensure this function is called only once.
    pub unsafe fn init(&self) {
        self.buf.store(
            alloc::alloc(alloc::Layout::new::<[u8; CONSOLE_BUFSIZE]>()),
            Ordering::Release,
        );
    }

    const fn new() -> ConsoleBuffer {
        ConsoleBuffer {
            buf: AtomicPtr::new(core::ptr::null_mut()),
            rpos: AtomicUsize::new(0),
            epos: AtomicUsize::new(0),
            wpos: AtomicUsize::new(0),
            read_lock: AtomicBool::new(false),
            write_lock: AtomicBool::new(false),
        }
    }

    pub fn recv_input(&self, c: u8) {
        let input_buf = self.buf.load(Ordering::Acquire);
        if input_buf.is_null() {
            return;
        }

        assert!(
            !self.write_lock.swap(true, Ordering::Acquire),
            "simultaneous write to console buffer"
        );

        let wpos = self.wpos.load(Ordering::Relaxed);
        let rpos = self.rpos.load(Ordering::Acquire);
        if wpos == rpos.checked_sub(1).unwrap_or(CONSOLE_BUFSIZE - 1) {
            // The buffer is full, ignore the character.
            self.write_lock.store(false, Ordering::Release);
            return;
        }

        unsafe {
            *input_buf.add(wpos) = c;
        }
        self.wpos
            .store((wpos + 1) % CONSOLE_BUFSIZE, Ordering::Release);

        self.write_lock.store(false, Ordering::Release);
    }

    unsafe fn _handle_echo(&self) {
        let input_buf = self.buf.load(Ordering::Acquire);
        if input_buf.is_null() {
            return;
        }

        let wpos = self.wpos.load(Ordering::Acquire);
        let mut epos = self.epos.load(Ordering::Relaxed);

        let mut serial = io::serial::COM1.take().expect("serial conflict");
        let mut cga = io::cga::CGA.take().expect("CGA conflict");

        while epos != wpos {
            let c = *input_buf.add(epos);
            serial.write_byte(c);
            cga.write_byte(c);
            epos = (epos + 1) % CONSOLE_BUFSIZE;
        }

        self.epos.store(epos, Ordering::Relaxed);
    }

    fn read(&self, buf: &mut [u8]) -> usize {
        let input_buf = self.buf.load(Ordering::Acquire);
        if input_buf.is_null() {
            return 0;
        }
        assert!(
            !self.read_lock.swap(true, Ordering::Acquire),
            "simultaneous read from console buffer"
        );
        unsafe {
            self._handle_echo();
        }

        let mut bufpos = 0;
        let wpos = self.wpos.load(Ordering::Acquire);
        let mut rpos = self.rpos.load(Ordering::Relaxed);

        while rpos != wpos && bufpos < buf.len() {
            unsafe {
                buf[bufpos] = *input_buf.add(rpos);
            }
            rpos = (rpos + 1) % CONSOLE_BUFSIZE;
            bufpos += 1;
        }

        self.rpos.store(rpos, Ordering::Release);
        self.read_lock.store(false, Ordering::Release);
        bufpos
    }

    fn can_read(&self) -> bool {
        assert!(
            !self.read_lock.swap(true, Ordering::Acquire),
            "simultaneous read from console buffer"
        );
        let can_read = self.rpos.load(Ordering::Relaxed) != self.wpos.load(Ordering::Acquire);
        self.read_lock.store(false, Ordering::Release);
        can_read
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

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ReadError> {
        Ok(CONSOLE_BUFFER.read(buf))
    }

    fn can_read(&mut self) -> bool {
        CONSOLE_BUFFER.can_read()
    }
}
