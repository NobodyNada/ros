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
pub mod fd;
pub mod heap;
pub mod prelude;
pub mod scheduler;
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

use crate::util::elfloader;
use crate::x86::mmu;

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
    mmu::MMU.take().unwrap().init();

    // Initialize input & handle any pending interrupts
    x86::io::serial::COM1.take().unwrap().enable_interrupts();
    x86::io::keyboard::KEYBOARD.take().unwrap().handle_input();
    unsafe {
        fd::CONSOLE_BUFFER.init();
    }

    x86::interrupt::sti();

    // Find and execute all elves
    // Look past the end of the kernel binary on disk for additional elves
    let mut offset = x86::io::pio::SECTOR_SIZE + // bootloader
                      unsafe { //kernel
                          core::ptr::addr_of!(mmu::KERNEL_VIRT_END)
                              .offset_from(core::ptr::addr_of!(mmu::KERNEL_VIRT_START)) as usize
                      };
    let mut scheduler: Option<scheduler::Scheduler> = None;
    let console = alloc::rc::Rc::new(core::cell::RefCell::new(fd::Console));
    while let Some(header) = elfloader::read_elf_headers(offset as u32).expect("I/O error") {
        kprintln!("Found ELF: {:#08x?}", header);

        // We've got an ELF header. Make an MMU environment for it
        let cr3 = {
            let mut mmu = mmu::MMU.take().unwrap();
            let mmu = &mut *mmu;

            match scheduler {
                None => {
                    // This is the first process, use the current MMU environment.
                    mmu.mapper.cr3()
                }
                Some(_) => mmu.mapper.fork(&mut mmu.allocator),
            }
        };

        // Load the ELF into memory
        header.load().expect("I/O error");
        offset = header.start_offset as usize + header.max_offset as usize;

        // allocate a 32-KiB user stack
        let user_stack_top = {
            let mut mmu = mmu::MMU.take().unwrap();
            let mmu = &mut *mmu;

            let user_stack_bytes = 0x8000;
            let user_stack_pages = user_stack_bytes >> mmu::PAGE_SHIFT;
            let user_stack = mmu
                .mapper
                .find_unused_userspace(user_stack_pages)
                .expect("not enough address space for user stack");
            mmu.mapper.map_zeroed(
                &mut mmu.allocator,
                user_stack,
                user_stack_pages,
                mmu::mmap::MappingFlags::new()
                    .with_writable(true)
                    .with_user_accessible(true),
            );

            user_stack + user_stack_bytes
        };

        // Add the process to the scheduler.
        let env = x86::env::Env {
            cr3,
            trap_frame: x86::interrupt::InterruptFrame {
                eip: header.entrypoint,
                cs: mmu::SegmentId::UserCode as usize,
                ds: mmu::SegmentId::UserData as usize,
                es: mmu::SegmentId::UserData as usize,
                fs: mmu::SegmentId::UserData as usize,
                gs: mmu::SegmentId::UserData as usize,
                user_ss: mmu::SegmentId::UserData as usize,
                user_esp: user_stack_top,
                eflags: 0x200, // enable interrupts
                ..Default::default()
            },
        };

        let pid = match scheduler.as_mut() {
            None => {
                scheduler = Some(scheduler::Scheduler::new(env));
                scheduler.as_mut().unwrap().current_pid()
            }
            Some(scheduler) => scheduler.add_process(env),
        };

        // set up stdio descriptors
        scheduler.as_mut().unwrap().set_fd(pid, 0, console.clone());
        scheduler.as_mut().unwrap().set_fd(pid, 1, console.clone());
        scheduler.as_mut().unwrap().set_fd(pid, 2, console.clone());
    }

    kprintln!("entering userland!");
    scheduler.expect("no processes found").run()
}
