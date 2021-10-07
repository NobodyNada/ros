#![allow(clippy::identity_op)]
#![allow(dead_code)]
use core::marker::PhantomData;

use crate::{util::Global, x86};
use modular_bitfield::prelude::*;

/// Disables interrupts.
pub fn cli() {
    unsafe {
        asm!("cli");
    }
}

/// Enables interrupts.
pub fn sti() {
    unsafe {
        asm!("sti");
    }
}

pub static IDT: Global<InterruptDescriptorTable> = Global::lazy_default();

#[repr(C)]
#[derive(Debug)]
pub struct InterruptFrame {
    eip: u32,
    cs: u32,
    eflags: u32,
    esp: u32,
    ss: u32,
}

#[bitfield]
#[repr(u64)]
#[derive(Copy, Clone)]
struct InterruptGate {
    offset_lo: B16,
    segment: B16,
    #[skip]
    __: B5,
    magic: B8,
    dpl: B2,
    present: bool,
    offset_hi: B16,
}
impl InterruptGate {
    pub unsafe fn create(offset: u32, segment: u16, dpl: u8, is_trap: bool) -> Self {
        let result = Self::new()
            .with_offset_hi((offset >> 16) as u16)
            .with_offset_lo(offset as u16)
            .with_segment(segment)
            .with_dpl(dpl)
            .with_present(true);
        if is_trap {
            result.with_magic(0b01111000)
        } else {
            result.with_magic(0b01110000)
        }
    }
}

pub unsafe trait InterruptHandler {
    fn to_offset(&self) -> u32;
}
type NoErr = extern "x86-interrupt" fn(InterruptFrame);
type WithErr = extern "x86-interrupt" fn(InterruptFrame, u32);

unsafe impl InterruptHandler for NoErr {
    fn to_offset(&self) -> u32 {
        (*self as usize) as u32
    }
}
unsafe impl InterruptHandler for WithErr {
    fn to_offset(&self) -> u32 {
        (*self as usize) as u32
    }
}

#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct Interrupt<T: InterruptHandler>(InterruptGate, core::marker::PhantomData<T>);

impl<T: InterruptHandler> Interrupt<T> {
    unsafe fn create(offset: u32, segment: u16, dpl: u8, is_trap: bool) -> Self {
        Self(
            InterruptGate::create(offset, segment, dpl, is_trap),
            PhantomData::default(),
        )
    }

    pub fn undefined() -> Self {
        Self(InterruptGate::new(), PhantomData::default())
    }

    /// Creates an interrupt gate descriptor for a hardware interrupt.
    /// The interrupt's DPL is set to 0, so it cannot be triggered by userspace software.
    pub fn hw_interrupt(isr: T) -> Self {
        unsafe { Self::create(isr.to_offset(), x86::cs(), 0, false) }
    }

    /// Creates an interrupt gate descriptor for a software interrupt (like a system call).
    /// The interrupt's DPL is set to 3, so it can be triggered by userspace software.
    pub fn sw_interrupt(isr: T) -> Self {
        unsafe { Self::create(isr.to_offset(), x86::cs(), 3, false) }
    }

    /// Creates an interrupt gate descriptor for a hardware trap.
    /// The interrupt's DPL is set to 0, so it cannot be triggered by userspace software.
    /// The trap bit is set, so the ISR can itself be interrupted.
    pub fn trap(isr: T) -> Self {
        unsafe { Self::create(isr.to_offset(), x86::cs(), 0, true) }
    }

    /// Creates an interrupt gate descriptor for a software trap.
    /// The interrupt's DPL is set to 3, so it can be triggered by userspace software.
    /// The trap bit is set, so the ISR can itself be interrupted.
    pub fn sw_trap(isr: T) -> Self {
        unsafe { Self::create(isr.to_offset(), x86::cs(), 3, true) }
    }
}

impl<T: InterruptHandler> Default for Interrupt<T> {
    fn default() -> Self {
        Self::undefined()
    }
}

#[repr(C)]
pub struct InterruptDescriptorTable {
    pub divide_error: Interrupt<NoErr>,                   // 0
    pub debug_exception: Interrupt<NoErr>,                // 1
    pub nmi: Interrupt<NoErr>,                            // 2
    pub breakpoint: Interrupt<NoErr>,                     // 3
    pub overflow: Interrupt<NoErr>,                       // 4
    pub bound_range: Interrupt<NoErr>,                    // 5
    pub invalid_opcode: Interrupt<NoErr>,                 // 6
    pub device_not_available: Interrupt<NoErr>,           // 7
    pub double_fault: Interrupt<WithErr>,                 // 8
    pub coprocessor_segment: Interrupt<NoErr>,            // 9
    pub invalid_tss: Interrupt<WithErr>,                  // 10
    pub segment_not_present: Interrupt<WithErr>,          // 11
    pub stack_segment_fault: Interrupt<WithErr>,          // 12
    pub general_protection_fault: Interrupt<WithErr>,     // 13
    pub page_fault: Interrupt<WithErr>,                   // 14
    pub _reserved: Interrupt<NoErr>,                      // 15
    pub math_fault: Interrupt<NoErr>,                     // 16
    pub alignment_check: Interrupt<WithErr>,              // 17
    pub machine_check: Interrupt<NoErr>,                  // 18
    pub simd_exception: Interrupt<NoErr>,                 // 19
    pub virtualization_exception: Interrupt<NoErr>,       // 20
    pub control_protection_exception: Interrupt<WithErr>, // 21
    pub _reserved2: [Interrupt<NoErr>; 10],               // 22-31
    pub user: [Interrupt<NoErr>; 224],                    // 32-255
}

impl Default for InterruptDescriptorTable {
    fn default() -> Self {
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
            general_protection_fault: Interrupt::undefined(),
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
}

impl InterruptDescriptorTable {
    /// Loads an interrupt descriptor table.
    pub fn lidt(&'static self) {
        #[repr(C)]
        struct Idtr {
            padding: u16,
            size: u16,
            paddr: u32,
        }

        let idtr = Idtr {
            padding: 0,
            size: 256 - 1,
            // TODO: this will be harder once we have "real" virtual memory
            paddr: ((self as *const _) as u32) & !0xf0000000,
        };

        unsafe {
            asm!("lidt [{}]", in(reg) core::ptr::addr_of!(idtr.size), options(nostack));
        }
    }
}

extern "x86-interrupt" fn double_fault(frame: InterruptFrame, _code: u32) {
    panic!("Double fault: {:#x?}", frame)
}
