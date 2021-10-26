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

use core::{fmt::Write, ops::DerefMut};
use x86::io::{cga, serial};

use crate::x86::mmu::{self, mmap::MappingFlags};

// Include assembly modules
global_asm!(include_str!("boot.asm"));
global_asm!(include_str!("kentry.asm"));

// Include Rust modules
pub mod debug;
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

/// Rust entrypoint (called by kentry.asm).
#[no_mangle]
pub extern "C" fn main() -> ! {
    kprintln!("good morning!");

    let idt = x86::interrupt::IDT.take_and_leak().unwrap();
    idt.lidt();

    let cow_test_1: &mut [u8];
    let cow_test_2: &mut [u8];
    let zeroed_test: &mut [u8];

    unsafe {
        let mut mmu = mmu::MMU.take().unwrap();
        let mmu = mmu.deref_mut();
        mmu.init();
        //kprintln!("Virtual memory mappings: {:#08x?}", mmu.get_mapper());

        let mut heap_vaddr = mmu.allocator.get_max_allocated() | mmu::KERNEL_RELOC_BASE as usize;

        // Allocate some pages to test the allocator and MMU
        let page1_paddr = mmu.allocator.alloc().unwrap();
        let page1_vaddr = heap_vaddr;
        heap_vaddr += mmu::PAGE_SIZE;
        mmu.mapper.map(
            &mut mmu.allocator,
            page1_paddr,
            page1_vaddr,
            MappingFlags::new().with_writable(true),
        );

        // ensure we can read and write to the page
        let page1_ptr = page1_vaddr as *mut u32;
        *page1_ptr = 0x12345678;
        assert_eq!(*page1_ptr, 0x12345678);

        // now share it, and map it again
        mmu.allocator.share(page1_paddr, &mut mmu.mapper);
        let page1_vaddr2 = heap_vaddr;
        heap_vaddr += mmu::PAGE_SIZE;
        mmu.mapper.map(
            &mut mmu.allocator,
            page1_paddr,
            page1_vaddr2,
            MappingFlags::new().with_writable(true),
        );

        let page1_ptr2 = page1_vaddr2 as *mut u32;
        *page1_ptr2 = 0x23456789;
        assert_eq!(*page1_ptr, 0x23456789);

        // free one of the references
        mmu.allocator.free(page1_paddr, &mut mmu.mapper);

        // allocate a new page, and ensure we didn't reuse page1
        let page2_paddr = mmu.allocator.alloc().unwrap();
        let page2_vaddr = heap_vaddr;
        heap_vaddr += mmu::PAGE_SIZE;
        mmu.mapper.map(
            &mut mmu.allocator,
            page2_paddr,
            page2_vaddr,
            MappingFlags::new().with_writable(true),
        );
        assert_ne!(page1_paddr, page2_paddr);

        // free the other reference to page1
        mmu.allocator.free(page1_paddr, &mut mmu.mapper);

        // allocate a third page, and ensure we DID reuse page1
        // (since it should be at the top of the freelist)
        let page3_paddr = mmu.allocator.alloc().unwrap();
        let page3_vaddr = heap_vaddr;
        heap_vaddr += mmu::PAGE_SIZE;
        mmu.mapper.map(
            &mut mmu.allocator,
            page3_paddr,
            page3_vaddr,
            MappingFlags::new().with_writable(true),
        );
        assert_eq!(page1_paddr, page3_paddr);

        mmu.allocator.free(page2_paddr, &mut mmu.mapper);

        // mark page3 as COW
        core::ptr::write_bytes(page3_vaddr as *mut u8, 0, mmu::PAGE_SIZE);
        mmu.allocator.share_vaddr_cow(page3_vaddr, &mut mmu.mapper);
        let page3_vaddr2 = heap_vaddr;
        heap_vaddr += mmu::PAGE_SIZE;
        mmu.mapper.map(
            &mut mmu.allocator,
            page3_paddr,
            page3_vaddr2,
            MappingFlags::new().with_writable(false),
        );

        // allocate a couple new zeroed pages
        let zeroed_vaddr = heap_vaddr;
        mmu.mapper
            .map_zeroed(&mut mmu.allocator, zeroed_vaddr, 2, MappingFlags::new());

        cow_test_1 = core::slice::from_raw_parts_mut(page3_vaddr as _, mmu::PAGE_SIZE);
        cow_test_2 = core::slice::from_raw_parts_mut(page3_vaddr2 as _, mmu::PAGE_SIZE);
        zeroed_test = core::slice::from_raw_parts_mut(zeroed_vaddr as _, mmu::PAGE_SIZE * 2);
    }

    // Try out some pagefaults!
    cow_test_1[0] = 0x42;
    assert_eq!(cow_test_2[0], 0);
    cow_test_2[0] = 0x47;
    assert_eq!(cow_test_1[0], 0x42);
    assert_eq!(cow_test_2[0], 0x47);

    assert!(zeroed_test.iter().all(|&x| x == 0));

    zeroed_test[0] = 1;
    assert_eq!(zeroed_test[0], 1);
    assert!(zeroed_test[1..].iter().all(|&x| x == 0));

    kprintln!("\n\n");
    kprintln!("--------------------------------------------------------------------------");
    kprintln!("All tests passed, now let's dereference a null pointer to see if it panics");

    unsafe {
        // dereference 0x4, because dereferencing an actual null pointer
        // is undefined and could cause the compiler to do weird things
        // https://blog.llvm.org/2011/05/what-every-c-programmer-should-know_14.html
        *(0x4 as *mut u32) = 0x12345678;
    }

    panic!("it didn't panic, that's bad");
}
