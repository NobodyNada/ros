// Don't include the Rust standard library (which requires a libc & syscalls)
// Instead, we'll use the 'core' minimal standard library: https://doc.rust-lang.org/core/index.html
#![no_std]
// Don't emit the standard 'main' function (which expects a standard operating system environment).
#![no_main]
// Enable unstable (nightly-only) features that we need:
//   Inline assembly
#![feature(asm, global_asm)]
//   Some compile-time-constant computation features
//     (used for e.g. generating pagetables and I/O port definitions at compile time)
#![feature(const_generics_defaults, const_fn_trait_bound, const_fn_fn_ptr_basics)]

// Include assembly modules
global_asm!(include_str!("boot.asm"));
global_asm!(include_str!("kentry.asm"));

// Include Rust modules
pub mod util;
pub mod x86;

/// Loops forever.
#[allow(clippy::empty_loop)]
pub fn halt() -> ! {
    loop {}
}

#[panic_handler]
fn panic(_panic: &core::panic::PanicInfo<'_>) -> ! {
    // Called if our code `panic!`'s -- for instance, if an assertion or bounds-check fails.
    // Eventually, we can add debugging features such as error messages & backtraces,
    halt() // but for now we'll just halt.
}

/// Rust entrypoint (called by kentry.asm).
#[no_mangle]
pub extern "C" fn main() -> ! {
    x86::io::serial::COM1
        .take()
        .unwrap()
        .write_str("Hello, world!\n");
    halt()
}
