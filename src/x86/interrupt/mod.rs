#![allow(clippy::identity_op)]
#![allow(dead_code)]
use core::marker::PhantomData;

use crate::{util::Global, x86};
use modular_bitfield::prelude::*;

mod handlers;
pub mod pic;

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

/// Executes a closure with interrupts disabled.
pub fn with_interrupts_disabled<F: FnOnce() -> T, T>(f: F) -> T {
    unsafe {
        let mut eflags: u32;
        asm!("pushfd; pop {}", out(reg) eflags);
        cli();
        let result = f();
        asm!("push {}; popfd", in(reg) eflags);

        result
    }
}

pub const IRQ_OFFSET: usize = 0x20;

pub static IDT: Global<InterruptDescriptorTable> = Global::lazy_default();

#[repr(C)]
#[derive(Clone, Debug, Default)]
pub struct InterruptFrame {
    pub ds: usize,
    pub es: usize,
    pub fs: usize,
    pub gs: usize,

    pub edi: usize,
    pub esi: usize,
    pub ebp: usize,
    pub _esp: usize,
    pub ebx: usize,
    pub edx: usize,
    pub ecx: usize,
    pub eax: usize,

    pub id: usize,
    pub code: usize,
    // ---
    // Pushed by CPU
    pub eip: usize,
    pub cs: usize,
    pub eflags: usize,
    pub user_esp: usize,
    pub user_ss: usize,
}

impl InterruptFrame {
    pub fn is_userspace(&self) -> bool {
        (self.cs & 3) != 0
    }
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

#[macro_export]
macro_rules! isr_witherr {
    ($id:expr, $handler:expr) => {{
        #[naked]
        unsafe extern "x86-interrupt" fn isr() {
            asm!(
                "push dword ptr {id}",
                "pushad",

                "mov ax, gs",
                "push eax",
                "mov ax, fs",
                "push eax",
                "mov ax, es",
                "push eax",
                "mov ax, ds",
                "push eax",

                "mov ax, {kdata}",
                "mov ds, ax",
                "mov es, ax",
                "mov fs, ax",
                "mov gs, ax",

                "mov eax, esp", // save pointer to interrupt frame

                // push the return address & EBP again, so that backtraces will work through interrupts
                "push [eax+{eip}]",
                "push [eax+{ebp}]",
                "mov ebp, esp",

                "push eax",     // pass the interrupt frame as an argument to the handler
                "call {handler}",

                "add esp, 12", // pop interrupt frame argument, return address, and EIP

                "pop eax",
                "mov ds, ax",
                "pop eax",
                "mov es, ax",
                "pop eax",
                "mov fs, ax",
                "pop eax",
                "mov gs, ax",

                "popad",
                "add esp, 8", // pop interrupt ID and code
                "iretd",
                id = const $id,
                kdata = const (crate::x86::mmu::SegmentId::KernelData as u16),
                eip = const 0x38, // offsetof(TrapFrame.eip)
                ebp = const 0x18, // offsetof(TrapFrame.ebp)
                handler = sym $handler,
                options(noreturn)
            );
        }
        $crate::x86::interrupt::WithErr(isr)
    }};
}

#[macro_export]
macro_rules! isr_noerr {
    ($id:expr, $handler:expr) => {{
        #[naked]
        unsafe extern "x86-interrupt" fn isr() {
            asm!(
                "push dword ptr 0", // push 0 for the code
                "push dword ptr {id}",
                "pushad",

                "mov ax, gs",
                "push eax",
                "mov ax, fs",
                "push eax",
                "mov ax, es",
                "push eax",
                "mov ax, ds",
                "push eax",

                "mov ax, {kdata}",
                "mov ds, ax",
                "mov es, ax",
                "mov fs, ax",
                "mov gs, ax",

                "mov eax, esp", // save pointer to interrupt frame

                // push the return address & EBP again, so that backtraces will work through interrupts
                "push [eax+{eip}]",
                "push [eax+{ebp}]",
                "mov ebp, esp",

                "push eax",     // pass the interrupt frame as an argument to the handler
                "call {handler}",

                "add esp, 12", // pop interrupt frame argument, return address, and EIP

                "pop eax",
                "mov ds, ax",
                "pop eax",
                "mov es, ax",
                "pop eax",
                "mov fs, ax",
                "pop eax",
                "mov gs, ax",

                "popad",
                "add esp, 8", // pop interrupt ID & code
                "iretd",
                id = const $id,
                kdata = const (crate::x86::mmu::SegmentId::KernelData as u16),
                eip = const 0x38, // offsetof(TrapFrame.eip)
                ebp = const 0x18, // offsetof(TrapFrame.ebp)
                handler = sym $handler,
                options(noreturn)
            );
        }
        $crate::x86::interrupt::NoErr(isr)
    }};
}

#[allow(clippy::missing_safety_doc)]
pub unsafe trait InterruptHandler: Copy {
    fn to_offset(&self) -> usize;
}

#[derive(Copy, Clone)]
pub struct NoErr(unsafe extern "x86-interrupt" fn());

#[derive(Copy, Clone)]
pub struct WithErr(unsafe extern "x86-interrupt" fn());

unsafe impl InterruptHandler for NoErr {
    fn to_offset(&self) -> usize {
        self.0 as usize
    }
}
unsafe impl InterruptHandler for WithErr {
    fn to_offset(&self) -> usize {
        self.0 as usize
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
        x86::DescriptorTableRegister::new(
            (core::mem::size_of_val(self) - 1) as u16,
            (self as *const _) as usize,
        )
        .lidt();
    }
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct InterruptNum(usize);

impl InterruptNum {
    const RESERVED_1: usize = 15;
    const HW_MAX: usize = HwInterruptNum::ControlProtectionException as usize;
    const USER_MIN: usize = 32;
    const USER_MAX: usize = 255;

    pub fn is_hw(&self) -> bool {
        match self.0 {
            Self::RESERVED_1 => false,
            0..=Self::HW_MAX => true,
            _ => false,
        }
    }

    pub fn is_user(&self) -> bool {
        (Self::USER_MIN..=Self::USER_MAX).contains(&self.0)
    }

    fn get_hw(&self) -> Option<HwInterruptNum> {
        if self.is_hw() {
            Some(unsafe { core::mem::transmute(self) })
        } else {
            None
        }
    }
}

#[derive(Debug, Copy, Clone)]
#[repr(usize)]
pub enum HwInterruptNum {
    DivideError = 0,
    DebugException = 1,
    NMI = 2,
    Breakpoint = 3,
    Overflow = 4,
    BoundRange = 5,
    InvalidOpcode = 6,
    DeviceNotAvailable = 7,
    DoubleFault = 8,
    CoprocessorSegment = 9,
    InvalidTSS = 10,
    SegmentNotPresent = 11,
    StackSegmentFault = 12,
    GeneralProtectionFault = 13,
    PageFault = 14,
    // 15 is reserved
    MathFault = 16,
    AlignmentCheck = 17,
    MachineCheck = 18,
    SimdException = 19,
    VirtualizationException = 20,
    ControlProtectionException = 21,
}
