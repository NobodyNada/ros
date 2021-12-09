#[repr(u8)]
pub enum SyscallId {
    Exit,
    YieldCpu,
    Read,
    Write,
    Close,
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

pub enum ReadError {
    BadFd,
    Unsupported,
}

pub enum WriteError {
    BadFd,
    Unsupported,
}
