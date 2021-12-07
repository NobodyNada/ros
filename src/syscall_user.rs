use crate::syscall::*;

fn syscall<A, R>(id: SyscallId, arg: &A) -> R {
    let mut result = core::mem::MaybeUninit::<R>::uninit();
    unsafe {
        asm!("int 0x20", in("al") id as u8, in("ebx") arg, in("ecx") &mut result);
        result.assume_init()
    }
}

pub fn exit() -> ! {
    syscall(SyscallId::Exit, &())
}

pub fn yield_cpu() {
    syscall(SyscallId::YieldCpu, &())
}

pub fn puts(s: &str) {
    syscall(SyscallId::Puts, &s)
}
