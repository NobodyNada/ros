use crate::syscall::*;
use core::arch::asm;

/// Terminates the current process.
pub fn exit() -> ! {
    syscall(SyscallId::Exit, &())
}

/// Yields the CPU, transferring the current process's timeslice to another process.
pub fn yield_cpu() {
    syscall(SyscallId::YieldCpu, &())
}

/// Attemps to read up to `buf.len()` bytes from a file descriptor, returning the number of bytes
/// actually read. Blocks if no data is available; returns 0 if the end-of-file is reached.
pub fn read(fd: Fd, buf: &mut [u8]) -> Result<usize, ReadError> {
    syscall(SyscallId::Read, &ReadArg { fd, buf })
}

/// Attemps to write up to `buf.len()` bytes from a file descriptor, returning the number of bytes
/// actually written. Blocks if no space is available.
pub fn write(fd: Fd, buf: &[u8]) -> Result<usize, WriteError> {
    syscall(SyscallId::Write, &WriteArg { fd, buf })
}

/// Closes a file descriptor. If the file descriptor does not exist, this is a no-op.
pub fn close(fd: Fd) {
    syscall(SyscallId::Close, &fd)
}

/// Opens a pipe, returning a read half and a write half.
/// Data written into the write half can be read out the read half.
pub fn pipe() -> (Fd, Fd) {
    syscall(SyscallId::Pipe, &())
}

/// Duplicates the current process, returning 0 to the child and the child's PID to the parent.
pub fn fork() -> Pid {
    syscall(SyscallId::Fork, &())
}

/// Replaces the current process with a new executable.
#[must_use]
pub fn exec(process: u32) -> ExecError {
    syscall(SyscallId::Exec, &process)
}

/// Blocks until the specified process terminates.
pub fn wait(process: Pid) {
    syscall(SyscallId::Wait, &process)
}

/// Duplicates a file descriptor.
pub fn dup2(src: Fd, dst: Fd) {
    syscall(SyscallId::Dup2, &(src, dst))
}

/// Creates and returns a null file descriptor.
/// The file descriptor will discard any data written to it and return EOF on reads.
pub fn null_fd() -> Fd {
    syscall(SyscallId::NullFd, &())
}

fn syscall<A, R>(id: SyscallId, arg: &A) -> R {
    let mut result = core::mem::MaybeUninit::<R>::uninit();
    unsafe {
        asm!("int 0x40", in("al") id as u8, in("ebx") arg, in("ecx") &mut result);
        result.assume_init()
    }
}
