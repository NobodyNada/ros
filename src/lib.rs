#![no_std]
#![feature(asm)]
#![feature(lang_items)]

// export syscall_common and syscall_user as syscall
mod syscall_common;
mod syscall_user;
pub mod syscall {
    //! The syscall module is defined in two files: syscall_common.rs and syscall_user.rs. Common
    //! contains data structures and definitions used by both userspace and kernelspace, whereas
    //! User contains only those used by uernelspace.
    pub use super::{syscall_common::*, syscall_user::*};
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::puts("user panicked\n");
    syscall::exit()
}

#[lang = "start"]
fn lang_start<T>(main: fn() -> T, _argc: isize, _argv: *const *const u8) -> isize {
    main();
    syscall::exit()
}
