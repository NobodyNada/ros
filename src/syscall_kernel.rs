//! Kernel-side syscall handlers

use core::ops::Deref;

use crate::syscall_common::*;
use crate::{
    kprintln, scheduler,
    x86::{interrupt, mmu},
};

/// Syscall interrupt handler
pub fn syscall(frame: &mut interrupt::InterruptFrame) {
    // al == syscall number
    let matched = match_syscall(frame, SyscallId::Exit, |frame, _: &()| exit(frame))
        || match_syscall(frame, SyscallId::YieldCpu, |frame, _: &()| yield_cpu(frame))
        || match_syscall(frame, SyscallId::Puts, |frame, s: &&str| puts(frame, *s));

    // If no syscall matched, panic
    // TODO: kill userspace process instead
    assert!(matched, "invalid syscall");
}

fn exit(frame: &mut interrupt::InterruptFrame) {
    let mut scheduler = scheduler::SCHEDULER.take().unwrap();
    let scheduler = scheduler.as_mut().unwrap();
    kprintln!("Process {} exited.", scheduler.current_pid());
    scheduler.kill_current_process(frame);
}

fn yield_cpu(frame: &mut interrupt::InterruptFrame) {
    let mut scheduler = scheduler::SCHEDULER.take().unwrap();
    let scheduler = scheduler.as_mut().unwrap();

    scheduler.schedule(frame);
}

fn puts(_frame: &mut interrupt::InterruptFrame, text: &str) {
    let pid = scheduler::SCHEDULER
        .take()
        .unwrap()
        .as_ref()
        .unwrap()
        .current_pid();
    kprintln!("[{}]: {}", pid, text)
}

/// Defines a type that can be safely passed between kernelspace and userspace.
trait Arg {
    /// Verifies that the pointer points to a valid instance of the type.
    /// The pointer is guaranteed to point to valid memory and be properly aligned.
    unsafe fn validate(arg: *const Self) -> bool;
}
impl Arg for () {
    unsafe fn validate(_arg: *const Self) -> bool {
        // every instance of void is valid
        true
    }
}

/// Validates that an address range points to valid, userspace-accessible memory.
/// If is_write is true, the memory must also be writable.
fn validate_range(start: usize, len: usize, is_write: bool) -> bool {
    let mmu = mmu::MMU.take().unwrap();
    let mmu = mmu.deref();

    mmu.mapper.validate_range(
        &mmu.allocator,
        start,
        len,
        mmu::mmap::MappingFlags::new().with_writable(is_write),
    )
}
// Validates that a pointer points to valid, userspace-accssible memory.
/// If is_write is true, the memory must also be writable.
fn validate_ptr<T: ?Sized>(ptr: *const T, is_write: bool) -> bool {
    unsafe {
        let addr = ptr as *const () as usize;
        addr % core::mem::align_of_val_raw(ptr) == 0
            && validate_range(
                ptr as *const () as usize,
                core::mem::size_of_val_raw(ptr),
                is_write,
            )
    }
}
impl<T: Arg + ?Sized> Arg for &T {
    unsafe fn validate(arg: *const Self) -> bool {
        // A reference is valid if both the pointer and pointee are valid
        let ptr = *(arg as *const *const T);
        validate_ptr(ptr, false) && T::validate(ptr)
    }
}
impl<T: Arg + ?Sized> Arg for &mut T {
    unsafe fn validate(arg: *const Self) -> bool {
        // A reference is valid if both the pointer and pointee are valid
        let ptr = *(arg as *const *const T);
        validate_ptr(ptr, true) && T::validate(ptr)
    }
}
impl<T: Arg> Arg for [T] {
    unsafe fn validate(arg: *const Self) -> bool {
        // A slice is valid if each element is valid
        (0..arg.len())
            .map(|i| T::validate(arg.get_unchecked(i)))
            .all(|is_valid| is_valid)
    }
}

impl Arg for u8 {
    unsafe fn validate(_arg: *const Self) -> bool {
        // Every u8 is valid
        true
    }
}
impl Arg for &str {
    unsafe fn validate(arg: *const Self) -> bool {
        // A string slice is valid if the byte slice points to valid memory
        // and contains valid UTF-8
        let byte_slice = arg.cast::<&[u8]>();
        <&[u8]>::validate(byte_slice) && core::str::from_utf8(*byte_slice).is_ok()
    }
}

/// If the syscall ID passed by the user process in `frame` matches `id`, decodes and validates the
/// arguments and invokes the syscall handler.
fn match_syscall<A: Arg, T: Arg>(
    frame: &mut interrupt::InterruptFrame,
    id: SyscallId,
    func: fn(&mut interrupt::InterruptFrame, &A) -> T,
) -> bool {
    if frame.eax as u8 == id as u8 {
        let arg_ptr = frame.ebx as *const A;
        let result_ptr = frame.ecx as *mut T;
        unsafe {
            assert!(A::validate(arg_ptr), "invalid syscall arg");
            assert!(
                validate_ptr(result_ptr, true),
                "invalid syscall result buffer"
            );
            let result = func(frame, arg_ptr.as_ref().unwrap());
            result_ptr.write(result);
        }
        true
    } else {
        false
    }
}
