use core::ops::DerefMut;

use crate::x86::{interrupt, mmu};
use modular_bitfield::prelude::*;

#[allow(dead_code, clippy::identity_op)]
pub extern "x86-interrupt" fn page_fault(frame: interrupt::InterruptFrame, code: usize) {
    #[repr(u32)]
    #[bitfield]
    #[derive(Debug)]
    struct PageFaultCode {
        present: bool,
        write: bool,
        user: bool,

        // NOTE: this bit isn't itself reserved; it indicates that we mistakenly
        // set a reserved bit somewhere in a paging structure
        reserved: bool,

        instruction_fetch: bool,
        protection_key_violated: bool,
        shadow_stack: bool,

        #[skip]
        __: B8,

        sgx: bool,

        #[skip]
        __: B16,
    }

    let mut vaddr: usize;
    unsafe {
        asm!("mov {}, cr2", out(reg) vaddr, options(nomem, nostack));
    }
    let code = PageFaultCode::from(code as u32);

    let unhandled = |msg| {
        panic!(
            "{} accessing virtual address {:#08x}\nCode: {:#08x?}\nFrame: {:#08x?}",
            msg, vaddr, code, frame
        )
    };

    let mut mmu = mmu::MMU
        .take()
        .unwrap_or_else(|| unhandled("page fault in MMU"));
    let mmu = mmu.deref_mut();

    let handled = if code.present() {
        let mapping = mmu.get_mapper().get_mapping(vaddr).unwrap();
        if !mapping.userspace_accessible() && code.user() {
            // insufficent permissions
            false
        } else if !mapping.is_writable() && code.write() {
            // write to read-only page
            // if the page was COW, then we've handled the pagefault
            mmu.mapper.cow_if_needed(&mut mmu.allocator, vaddr)
        } else {
            false
        }
    } else {
        false
    };

    if !handled {
        unhandled("Unhandled pagefault");
    }
}
