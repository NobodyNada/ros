#[repr(u8)]
/// A syscall number.
pub enum SyscallId {
    Exit,
    YieldCpu,
    Read,
    Write,
    Close,
    Pipe,
    Fork,
    Exec,
    Wait,
    Dup2,
    NullFd,
}

pub type Fd = u32;
pub type Pid = u32;

/// An argument to the 'read' syscall.
pub struct ReadArg<'a> {
    pub fd: Fd,
    pub buf: &'a mut [u8],
}

/// An argument to the 'write' syscall.
pub struct WriteArg<'a> {
    pub fd: Fd,
    pub buf: &'a [u8],
}

/// An error returned by the 'read' syscall.
#[derive(Debug)]
pub enum ReadError {
    /// The file descriptor does not exist.
    BadFd,
    /// The file descriptor does not support reading.
    Unsupported,
}

#[derive(Debug)]
pub enum WriteError {
    /// The file descriptor does not exist.
    BadFd,
    /// The file descriptor does not support writing.
    Unsupported,
}

/// An error returned by the 'exec' syscall.
#[derive(Debug)]
pub enum ExecError {
    /// The executable does not exist.
    BadProcess,

    /// Could not read the executable from disk.
    IoError,
}
