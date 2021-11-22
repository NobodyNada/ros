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
#![feature(allocator_api, alloc_error_handler)]

extern crate alloc;

use core::fmt::Write;
use x86::io::{cga, serial};

use crate::util::elfloader;
use crate::x86::mmu;

// Include assembly modules
global_asm!(include_str!("boot.asm"));
global_asm!(include_str!("kentry.asm"));

// Include Rust modules
pub mod debug;
pub mod heap;
pub mod prelude;
pub mod scheduler;
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
    kprintln!("good morning, that's a nice tnettenba");

    let idt = x86::interrupt::IDT.take_and_leak().unwrap();
    idt.lidt();
    mmu::MMU.take().unwrap().init();

    // find and execute all elves
    let mut offset = x86::io::pio::SECTOR_SIZE + // bootloader
                      unsafe { //kernel
                          core::ptr::addr_of!(mmu::KERNEL_VIRT_END)
                              .offset_from(core::ptr::addr_of!(mmu::KERNEL_VIRT_START)) as usize
                      };
    let mut scheduler: Option<scheduler::Scheduler> = None;
    while let Some(header) = elfloader::read_elf_headers(offset as u32).expect("I/O error") {
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

        kprintln!("Found ELF: {:#08x?}", header);
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
            kprintln!("user stack at {:#08x?}", user_stack);

            kprintln!("Memory mappings: {:#08x?}", mmu.mapper);

            user_stack + (user_stack_bytes - 1)
        };

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
                ..Default::default()
            },
        };

        // Add the process to the scheduler.
        match scheduler.as_mut() {
            None => scheduler = Some(scheduler::Scheduler::new(env)),
            Some(scheduler) => {
                scheduler.add_process(env);
            }
        };
    }

    kprintln!("entering userland!");
    scheduler.expect("no processes found").run()
}
