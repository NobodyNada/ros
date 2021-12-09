//! ROS userland runtime library
//!
//! Defines data structures and functions ROS programs can use to communicate with the kernel.

pub mod io;

pub use crate::syscall;

/// Userland panic handler
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = crate::fprintln!(&mut io::stderr(), "user process panicked: {}", info);
    syscall::exit()
}

/// Rust runtime entry point, equivalent to the '_start' function on a Unix-like operating system
#[lang = "start"]
fn lang_start<T>(main: fn() -> T, _argc: isize, _argv: *const *const u8) -> isize {
    main();
    syscall::exit()
}
