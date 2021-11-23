#![allow(clippy::identity_op, dead_code)]
use core::ops::DerefMut;

use crate::{
    kprintln,
    util::Global,
    x86::{interrupt, mmu},
};
use modular_bitfield::prelude::*;

pub type PageFaultHandler =
    &'static mut dyn FnMut(&mut interrupt::InterruptFrame, usize, PageFaultCode) -> bool;
pub static PAGE_FAULT_HANDLER: Global<Option<PageFaultHandler>> = Global::new(None);

/// Executes some code with a pagefault handler installed.
pub fn with_page_fault_handler<F: FnOnce() -> T, T>(
    f: F,
    handler: &mut dyn FnMut(&mut interrupt::InterruptFrame, usize, PageFaultCode) -> bool,
) -> T {
    // Coerce the lifetime of handler to 'static so that we can store it in a global variable
    // (temporarily). We are responsible for ensuring the actual lifetime is respected -- i.e.
    // that handler does not outlive this function.
    let handler = unsafe { core::mem::transmute(handler) };

    let old = PAGE_FAULT_HANDLER.take().unwrap().replace(handler);
    let result = f();
    *PAGE_FAULT_HANDLER.take().unwrap() = old;

    result
}

/// Attempts to read from a virtual address, returning an error if the access pagefaults.
pub fn try_read(vaddr: *const usize) -> Result<usize, PageFaultCode> {
    let mut val: usize = 0;
    let mut code: Option<PageFaultCode> = None;
    with_page_fault_handler(
        || unsafe {
            asm!(
                "lea eax, 1f",
                // Attempt to read the virtual address. If the read faults, the page fault handler
                // will set eip to [eax] (the 1: label at the end of the asm block) and skip the
                // read.
                "mov eax, [{vaddr}]",
                "1:",
                vaddr = in(reg) vaddr, out("eax") val, options(nostack));
        },
        &mut |frame, fault_addr, fault_code| {
            let range = (vaddr as usize)..(vaddr as usize + core::mem::size_of::<usize>());
            if range.contains(&fault_addr) {
                code = Some(fault_code);
                frame.eip = frame.eax;
                true
            } else {
                false
            }
        },
    );

    match code {
        None => Ok(val),
        Some(code) => Err(code),
    }
}

#[repr(u32)]
#[bitfield]
#[derive(Debug, Copy, Clone)]
pub struct PageFaultCode {
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

#[allow(dead_code, clippy::identity_op)]
pub extern "C" fn page_fault(frame: &mut interrupt::InterruptFrame) {
    let mut vaddr: usize;
    unsafe {
        asm!("mov {}, cr2", out(reg) vaddr, options(nomem, nostack));
    }
    let code = PageFaultCode::from(frame.code as u32);

    // If we have a custom page fault handler, call it first.
    let handled = PAGE_FAULT_HANDLER
        .take()
        .as_deref_mut()
        .unwrap_or(&mut None)
        .as_mut()
        .map(|handler| handler(frame, vaddr, code))
        .unwrap_or(false);

    let unhandled = |msg| {
        panic!(
            "{} accessing virtual address {:#08x}\nCode: {:#08x?}\nFrame: {:#08x?}",
            msg, vaddr, code, frame
        )
    };

    let handled = handled || {
        let mut mmu = mmu::MMU
            .take()
            .unwrap_or_else(|| unhandled("page fault in MMU"));
        let mmu = mmu.deref_mut();

        if code.present() {
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
        }
    };

    if !handled {
        if frame.is_userspace() {
            // Kill the offending process
            let mut scheduler = crate::scheduler::SCHEDULER.take().unwrap();
            let scheduler = scheduler.as_mut().unwrap();

            kprintln!(
                "terminating process {} due to unhandled pagefault",
                scheduler.current_pid()
            );

            scheduler.kill_current_process(frame);
        } else {
            unhandled("Unhandled pagefault");
        }
    }
}
