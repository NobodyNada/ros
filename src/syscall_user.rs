use crate::syscall::*;

fn syscall<A, R>(id: SyscallId, arg: &A) -> R {
    let mut result = core::mem::MaybeUninit::<R>::uninit();
    unsafe {
        asm!("int 0x40", in("al") id as u8, in("ebx") arg, in("ecx") &mut result);
        result.assume_init()
    }
}

pub fn exit() -> ! {
    syscall(SyscallId::Exit, &())
}

pub fn yield_cpu() {
    syscall(SyscallId::YieldCpu, &())
}

pub fn read(fd: Fd, buf: &mut [u8]) -> Result<usize, ReadError> {
    syscall(SyscallId::Read, &ReadArg { fd, buf })
}

pub fn write(fd: Fd, buf: &[u8]) -> Result<usize, WriteError> {
    syscall(SyscallId::Write, &WriteArg { fd, buf })
}

pub fn close(fd: Fd) {
    syscall(SyscallId::Close, &fd)
}

pub fn pipe() -> (Fd, Fd) {
    syscall(SyscallId::Pipe, &())
}
