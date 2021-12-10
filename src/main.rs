//! Crate definition and Rust entrypoint for ROS kernel.
//!
//! This file defines the module structure and language configuration for the ROS kernel binary. It
//! also contains the panic handler and the boot entrypoint.

// Configure language features as needed
// Don't include the Rust standard library (which requires a libc & syscalls)
// Instead, we'll use the 'core' minimal standard library: https://doc.rust-lang.org/core/index.html
#![no_std]
// Don't emit the standard 'main' function (which expects a standard operating system environment).
#![no_main]
// Enable unstable (nightly-only) features that we need:
//   Inline assembly
#![feature(asm, global_asm, asm_const, asm_sym, naked_functions)]
//   Compiler support for x86 interrupt calling conventions
#![feature(abi_x86_interrupt)]
//   PanicInfo::message() function
#![feature(panic_info_message)]
//   Some compile-time-constant computation features
//     (used for e.g. generating pagetables and I/O port definitions at compile time)
#![feature(const_generics_defaults, const_fn_trait_bound, const_fn_fn_ptr_basics)]
//   APIs for defining custom memory allocators
#![feature(allocator_api, alloc_error_handler)]
//   APIs for low-level manipulation of Rust objects, used to implement syscalls
#![feature(layout_for_ptr, slice_ptr_get, slice_ptr_len)]

// Link against the 'alloc' crate, which defines
// standard library data structures and collection
extern crate alloc;

// Include assembly modules
global_asm!(include_str!("boot.asm"));
global_asm!(include_str!("kentry.asm"));

// Include Rust modules
pub mod debug;
pub mod heap;
pub mod prelude;
pub mod process;
pub mod util;
pub mod x86;

// export syscall_common and syscall_kernel as syscall
mod syscall_common;
mod syscall_kernel;
pub mod syscall {
    //! The syscall module is defined in two files: syscall_common.rs and syscall_kernel.rs. Common
    //! contains data structures and definitions used by both userspace and kernelspace, whereas
    //! Kernel contains only those used by kernelspace.
    pub use super::{syscall_common::*, syscall_kernel::*};
}

use core::fmt::Write;
use x86::io::{cga, serial};

use crate::{
    process::{elfloader, scheduler},
    x86::mmu,
};

/// Halts the CPU by disabling interrupts and looping forever.
#[allow(clippy::empty_loop)]
#[no_mangle]
pub extern "C" fn halt() -> ! {
    x86::interrupt::cli();
    loop {}
}

/// Called if our code `panic!`'s -- for instance, if an assertion or bounds-check fails.
#[panic_handler]
unsafe fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
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
    if debug::backtrace(|frame| write_panic_message(format_args!(" {:#08x}", frame))).is_some() {
        write_panic_message(format_args!(" <page fault>"));
    }
    write_panic_message(format_args!("\n"));

    halt()
}

/// Called if dynamic memory allocation fails.
#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("allocation failed: {:#08x?}", layout)
}

/// Rust entrypoint (called by kentry.asm).
#[no_mangle]
#[allow(clippy::stable_sort_primitive)]
pub extern "C" fn main() -> ! {
    kprintln!("good morning, that's a nice tnettenba");

    // Initialize interrupts and MMU
    let idt = x86::interrupt::IDT.take_and_leak().unwrap();
    idt.lidt();
    let cr3 = {
        let mut mmu = mmu::MMU.take().unwrap();
        mmu.init();
        mmu.mapper.cr3()
    };

    // Initialize input & handle any pending interrupts
    x86::io::serial::COM1.take().unwrap().enable_interrupts();
    x86::io::keyboard::KEYBOARD.take().unwrap().handle_input();
    unsafe {
        process::fd::CONSOLE_BUFFER.init();
    }

    // Enable interrupts
    x86::interrupt::sti();

    let elves = elfloader::ELVES.get();
    kprintln!(
        "Found {} {}.",
        elves.len(),
        if elves.len() == 1 { "elf" } else { "elves" }
    );
    // Execute the first elf
    let trap_frame = elves
        .first()
        .expect("no elves found")
        .load()
        .expect("failed to load elf");
    let mut scheduler = scheduler::Scheduler::new(x86::env::Env { cr3, trap_frame });

    // set up stdio descriptors
    let console = alloc::rc::Rc::new(core::cell::RefCell::new(process::fd::Console));
    scheduler.set_fd(scheduler.current_pid(), 0, Some(console.clone()));
    scheduler.set_fd(scheduler.current_pid(), 1, Some(console.clone()));
    scheduler.set_fd(scheduler.current_pid(), 2, Some(console));

    kprintln!("entering userland!");
    scheduler.run()
}
