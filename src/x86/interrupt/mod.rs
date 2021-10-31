#![allow(clippy::identity_op)]
#![allow(dead_code)]
use core::marker::PhantomData;

use crate::{util::Global, x86};
use modular_bitfield::prelude::*;

mod handlers;

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
    eip: usize,
    cs: usize,
    eflags: usize,
    esp: usize,
    ss: usize,
}

#[bitfield]
#[repr(u64, align(8))]
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
    pub unsafe fn create(offset: usize, segment: u16, dpl: u8, is_trap: bool) -> Self {
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

#[allow(clippy::missing_safety_doc)]
pub unsafe trait InterruptHandler {
    fn to_offset(&self) -> usize;
}
type NoErr = extern "x86-interrupt" fn(InterruptFrame);
type WithErr = extern "x86-interrupt" fn(InterruptFrame, usize);

unsafe impl InterruptHandler for NoErr {
    fn to_offset(&self) -> usize {
        (*self as usize) as usize
    }
}
unsafe impl InterruptHandler for WithErr {
    fn to_offset(&self) -> usize {
        (*self as usize) as usize
    }
}

#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct Interrupt<T: InterruptHandler>(InterruptGate, core::marker::PhantomData<T>);

impl<T: InterruptHandler> Interrupt<T> {
    unsafe fn create(offset: usize, segment: u16, dpl: u8, is_trap: bool) -> Self {
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
        handlers::default()
    }
}

impl InterruptDescriptorTable {
    /// Loads an interrupt descriptor table.
    pub fn lidt(&'static self) {
        x86::DescriptorTableRegister::new(256 - 1, (self as *const _) as usize).lidt();
    }
}
