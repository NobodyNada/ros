use crate::x86::{interrupt::*, io, mmu};
use crate::{isr_noerr, isr_witherr};
use crate::{kprintln, syscall};

/// Populates and returns the IDT.
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

    idt.user[pit::Pit::IRQ] = Interrupt::hw_interrupt(isr_noerr!(
        pit::Pit::IRQ + IRQ_OFFSET,
        pit::Pit::handle_interrupt
    ));
    idt.user[io::keyboard::Keyboard::IRQ] = Interrupt::hw_interrupt(isr_noerr!(
        io::keyboard::Keyboard::IRQ + IRQ_OFFSET,
        io::keyboard::Keyboard::handle_interrupt
    ));
    idt.user[io::serial::COM1_IRQ] = Interrupt::hw_interrupt(isr_noerr!(
        io::serial::COM1_IRQ + IRQ_OFFSET,
        io::serial::Serial::<{ io::serial::COM1_BASE }>::handle_interrupt
    ));
    idt.user[pic::Pic::IRQ_SPURIOUS_MASTER] = Interrupt::hw_interrupt(isr_noerr!(
        pic::Pic::IRQ_SPURIOUS_MASTER + IRQ_OFFSET,
        pic::Pic::handle_spurious_master
    ));
    idt.user[pic::Pic::IRQ_SPURIOUS_SLAVE] = Interrupt::hw_interrupt(isr_noerr!(
        pic::Pic::IRQ_SPURIOUS_SLAVE + IRQ_OFFSET,
        pic::Pic::handle_spurious_slave
    ));
    idt.user[0x20] = Interrupt::sw_trap(isr_noerr!(0x20 + IRQ_OFFSET, syscall::syscall));

    idt
}

fn general_protection_fault(frame: &mut InterruptFrame) {
    if frame.is_userspace() {
        let continuation = {
            let mut scheduler = crate::process::scheduler::SCHEDULER.take().unwrap();
            let scheduler = scheduler.as_mut().unwrap();
            kprintln!(
                "terminating process {} due to general protection fault: {:#x?}",
                scheduler.current_pid(),
                frame
            );
            scheduler.kill_current_process(frame).1
        };
        continuation(frame);
    } else {
        panic!("General protection fault {:#x?}", frame)
    }
}

fn double_fault(frame: &mut InterruptFrame) {
    panic!("Double fault: {:#x?}", frame)
}
