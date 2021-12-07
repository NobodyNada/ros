use crate::syscall;
use crate::x86::{
    interrupt::{HwInterruptNum, Interrupt, InterruptDescriptorTable, InterruptFrame},
    mmu,
};
use crate::{isr_noerr, isr_witherr};

pub fn default() -> InterruptDescriptorTable {
    let mut idt = InterruptDescriptorTable {
        divide_error: Interrupt::undefined(),
        debug_exception: Interrupt::undefined(),
        nmi: Interrupt::undefined(),
        breakpoint: Interrupt::undefined(),
        overflow: Interrupt::undefined(),
        bound_range: Interrupt::undefined(),
        invalid_opcode: Interrupt::undefined(),
        device_not_available: Interrupt::undefined(),
        double_fault: Interrupt::hw_interrupt(isr_witherr!(
            HwInterruptNum::DoubleFault as usize,
            double_fault
        )),
        coprocessor_segment: Interrupt::undefined(),
        invalid_tss: Interrupt::undefined(),
        segment_not_present: Interrupt::undefined(),
        stack_segment_fault: Interrupt::undefined(),
        general_protection_fault: Interrupt::hw_interrupt(isr_witherr!(
            HwInterruptNum::GeneralProtectionFault as usize,
            general_protection_fault
        )),
        page_fault: Interrupt::hw_interrupt(isr_witherr!(
            HwInterruptNum::PageFault as usize,
            mmu::pagefault::page_fault
        )),
        _reserved: Interrupt::undefined(),
        math_fault: Interrupt::undefined(),
        alignment_check: Interrupt::undefined(),
        machine_check: Interrupt::undefined(),
        simd_exception: Interrupt::undefined(),
        virtualization_exception: Interrupt::undefined(),
        control_protection_exception: Interrupt::undefined(),
        _reserved2: [Interrupt::undefined(); 10],
        user: [Interrupt::undefined(); 224],
    };

    idt.user[0] = Interrupt::sw_trap(isr_noerr!(0x20, syscall::syscall));

    idt
}

fn general_protection_fault(frame: &mut InterruptFrame) {
    panic!("General protection fault {:#x?}", frame)
}

fn double_fault(frame: &mut InterruptFrame) {
    panic!("Double fault: {:#x?}", frame)
}
