#[repr(u8)]
pub enum SyscallId {
    Exit,
    YieldCpu,
    Read,
    Write,
    Close,
    Pipe,
}

pub type Fd = u32;

pub struct ReadArg<'a> {
    pub fd: Fd,
    pub buf: &'a mut [u8],
}

pub struct WriteArg<'a> {
    pub fd: Fd,
    pub buf: &'a [u8],
}

#[derive(Debug)]
pub enum ReadError {
    BadFd,
    Unsupported,
}

#[derive(Debug)]
pub enum WriteError {
    BadFd,
    Unsupported,
}
