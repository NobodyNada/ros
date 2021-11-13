// Don't include the Rust standard library (which requires a libc & syscalls)
// Instead, we'll use the 'core' minimal standard library: https://doc.rust-lang.org/core/index.html
#![no_std]
// Don't emit the standard 'main' function (which expects a standard operating system environment).
#![no_main]
// Enable unstable (nightly-only) features that we need:
//   Inline assembly
#![feature(asm, global_asm)]
//   Compiler support for x86 interrupt calling conventions
#![feature(abi_x86_interrupt)]
//   PanicInfo::message() function
#![feature(panic_info_message)]
//   Some compile-time-constant computation features
//     (used for e.g. generating pagetables and I/O port definitions at compile time)
#![feature(const_generics_defaults, const_fn_trait_bound, const_fn_fn_ptr_basics)]
#![feature(allocator_api, alloc_error_handler)]

extern crate alloc;

use core::fmt::Write;
use x86::io::{cga, serial};

use crate::x86::mmu;

// Include assembly modules
global_asm!(include_str!("boot.asm"));
global_asm!(include_str!("kentry.asm"));

// Include Rust modules
pub mod debug;
pub mod heap;
pub mod prelude;
pub mod util;
pub mod x86;

/// Loops forever.
#[allow(clippy::empty_loop)]
#[no_mangle]
pub extern "C" fn halt() -> ! {
    x86::interrupt::cli();
    loop {}
}

#[panic_handler]
unsafe fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    // Called if our code `panic!`'s -- for instance, if an assertion or bounds-check fails.
    x86::interrupt::cli();

    // Forcibly reset the serial port (even if someone else was using it)
    let mut serial = serial::Serial::<{ serial::COM1_BASE }>::new();

    let mut write_panic_message = |fmt: core::fmt::Arguments<'_>| {
        let _ = serial.write_fmt(fmt);
        // Also write to CGA, but ignore conflicts
        if let Some(mut cga) = cga::CGA.take() {
            let _ = cga.write_fmt(fmt);
        }
    };
    write_panic_message(format_args!("\n\npanic: {}\n", info));

    write_panic_message(format_args!("Stack trace:"));
    debug::backtrace(|frame| write_panic_message(format_args!(" {:#08x}", frame)));
    write_panic_message(format_args!("\n"));

    halt()
}

#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("allocation failed: {:#08x?}", layout)
}

/// Rust entrypoint (called by kentry.asm).
#[no_mangle]
#[allow(clippy::stable_sort_primitive)]
pub extern "C" fn main() -> ! {
    kprintln!("good morning!");

    let idt = x86::interrupt::IDT.take_and_leak().unwrap();
    idt.lidt();
    mmu::MMU.take().unwrap().init();

    let mut v = alloc::vec![4, 5, 3, 6, 4, 5, 1];
    kprintln!("alloced");
    v.sort();
    kprintln!("{:?}", v);

    let v2 = v.clone();
    core::mem::drop(v);
    core::mem::drop(v2);

    halt()
}
