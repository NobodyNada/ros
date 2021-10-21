use super::{Interrupt, InterruptDescriptorTable, InterruptFrame};

pub fn default() -> InterruptDescriptorTable {
    InterruptDescriptorTable {
        divide_error: Interrupt::undefined(),
        debug_exception: Interrupt::undefined(),
        nmi: Interrupt::undefined(),
        breakpoint: Interrupt::undefined(),
        overflow: Interrupt::undefined(),
        bound_range: Interrupt::undefined(),
        invalid_opcode: Interrupt::undefined(),
        device_not_available: Interrupt::undefined(),
        double_fault: Interrupt::hw_interrupt(double_fault),
        coprocessor_segment: Interrupt::undefined(),
        invalid_tss: Interrupt::undefined(),
        segment_not_present: Interrupt::undefined(),
        stack_segment_fault: Interrupt::undefined(),
        general_protection_fault: Interrupt::hw_interrupt(general_protection_fault),
        page_fault: Interrupt::undefined(),
        _reserved: Interrupt::undefined(),
        math_fault: Interrupt::undefined(),
        alignment_check: Interrupt::undefined(),
        machine_check: Interrupt::undefined(),
        simd_exception: Interrupt::undefined(),
        virtualization_exception: Interrupt::undefined(),
        control_protection_exception: Interrupt::undefined(),
        _reserved2: [Interrupt::undefined(); 10],
        user: [Interrupt::undefined(); 224],
    }
}

extern "x86-interrupt" fn general_protection_fault(frame: InterruptFrame, code: u32) {
    panic!("General protection fault {:#x?}: {:#x?}", code, frame)
}

extern "x86-interrupt" fn double_fault(frame: InterruptFrame, _code: u32) {
    panic!("Double fault: {:#x?}", frame)
}
