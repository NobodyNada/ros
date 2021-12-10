use ::alloc::{collections::VecDeque, rc::Rc};
use alloc::alloc;
use core::{
    cell::RefCell,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering},
};

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

/// A ring buffer to store incoming console bytes.  We have to be kinda careful when accessing
/// this, because it can be written asynchronously from an interrupt context.
const CONSOLE_BUFSIZE: usize = 4096;
pub struct ConsoleBuffer {
    /// The buffer; an array of length CONSOLE_BUFSIZE,
    /// or null if the console has not yet been initialized.
    buf: AtomicPtr<u8>,

    /// The position of the reader.
    rpos: AtomicUsize,

    /// The position of the local echo reader.
    epos: AtomicUsize,

    /// The position of the writer.
    wpos: AtomicUsize,

    /// True if the buffer is currently being read.
    read_lock: AtomicBool,

    /// True if the buffer is currently being written.
    write_lock: AtomicBool,
}
pub static CONSOLE_BUFFER: ConsoleBuffer = ConsoleBuffer::new();

impl ConsoleBuffer {
    /// Processes local echo.
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

    /// Recieves an input character. This function is meant to be called from an interrupt context.
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

pub struct Null;
impl File for Null {
    fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        Ok(buf.len())
    }

    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, ReadError> {
        Ok(0)
    }
}

// Pipes
// A pipe has two files associated with it: a read half and a write half. Both halves share a
// buffer using reference counting; the reference count of the buffer is therefore always 2 unless
// one half is closed.

/// The maximum buffer size for a pipe. If the buffer is full, writes will block.
pub const PIPE_BUF_LEN: usize = 1 << 16;

struct PipeRead {
    buf: Rc<RefCell<VecDeque<u8>>>,
}
impl File for PipeRead {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ReadError> {
        let mut src = self.buf.borrow_mut();
        if src.is_empty() && Rc::strong_count(&self.buf) == 1 {
            // The write half is closed and the buffer is empty, EOF.
            src.shrink_to_fit();
            Ok(0)
        } else {
            let count = core::cmp::min(buf.len(), src.len());
            buf.iter_mut()
                .zip(src.drain(0..count))
                .for_each(|(a, b)| *a = b);
            Ok(count)
        }
    }

    fn can_read(&mut self) -> bool {
        !self.buf.borrow().is_empty() || Rc::strong_count(&self.buf) == 1
    }
}

struct PipeWrite {
    buf: Rc<RefCell<VecDeque<u8>>>,
}
impl File for PipeWrite {
    fn write(&mut self, buf: &[u8]) -> Result<usize, WriteError> {
        let mut dst = self.buf.borrow_mut();
        if Rc::strong_count(&self.buf) == 1 {
            // The read half is closed, just discard everything.
            *dst = VecDeque::new(); // clear the buffer
            Ok(buf.len())
        } else {
            let count = core::cmp::min(buf.len(), PIPE_BUF_LEN - dst.len());
            buf[0..count].iter().for_each(|&x| dst.push_back(x));
            Ok(count)
        }
    }

    fn can_write(&mut self) -> bool {
        self.buf.borrow().len() != PIPE_BUF_LEN || Rc::strong_count(&self.buf) == 1
    }
}

/// Opens a new pipe, returning a read half and a write half.
/// Data written to the write half can be read out the read half.
pub fn pipe() -> (impl File, impl File) {
    let buf = Rc::new(RefCell::new(VecDeque::new()));
    (PipeRead { buf: buf.clone() }, PipeWrite { buf })
}
