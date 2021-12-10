//! Kernel-side syscall handlers

use core::cell::RefCell;
use core::ops::Deref;

use alloc::rc::Rc;

use crate::syscall_common::*;
use crate::{
    fd, kprintln, scheduler,
    x86::{interrupt, mmu},
};

/// Syscall interrupt handler
pub fn syscall(frame: &mut interrupt::InterruptFrame) {
    // al == syscall number
    let matched = match_syscall(frame, SyscallId::Exit, |frame, _: ()| exit(frame))
        || match_syscall(frame, SyscallId::YieldCpu, |frame, _: ()| yield_cpu(frame))
        || match_syscall_blocking(frame, SyscallId::Read, read)
        || match_syscall_blocking(frame, SyscallId::Write, write)
        || match_syscall(frame, SyscallId::Close, close)
        || match_syscall(frame, SyscallId::Pipe, |_, _: ()| pipe())
        || match_syscall(frame, SyscallId::Fork, |frame, _: ()| fork(frame));

    // If no syscall matched, panic
    // TODO: kill userspace process instead
    assert!(matched, "invalid syscall");
}

fn exit(frame: &mut interrupt::InterruptFrame) {
    let continuation = {
        let mut scheduler = scheduler::SCHEDULER.take().unwrap();
        let scheduler = scheduler.as_mut().unwrap();
        kprintln!("Process {} exited.", scheduler.current_pid());
        scheduler.kill_current_process(frame).1
    };

    continuation(frame);
}

fn yield_cpu(frame: &mut interrupt::InterruptFrame) {
    let continuation = {
        let mut scheduler = scheduler::SCHEDULER.take().unwrap();
        let scheduler = scheduler.as_mut().unwrap();
        scheduler.schedule(frame)
    };
    continuation(frame);
}

fn read(
    _frame: &mut interrupt::InterruptFrame,
    arg: ReadArg,
) -> Blocking<Result<usize, ReadError>> {
    let mut scheduler = scheduler::SCHEDULER.take().unwrap();
    let scheduler = scheduler.as_mut().unwrap();

    if let Some(fd) = scheduler.get_fd(scheduler.current_pid(), arg.fd) {
        let mut fd = fd.borrow_mut();
        if fd.can_read() {
            Ok(fd.read(arg.buf))
        } else {
            block(arg.fd, fd::AccessType::Read)
        }
    } else {
        Ok(Err(ReadError::BadFd))
    }
}

fn write(
    _frame: &mut interrupt::InterruptFrame,
    arg: WriteArg,
) -> Blocking<Result<usize, WriteError>> {
    let mut scheduler = scheduler::SCHEDULER.take().unwrap();
    let scheduler = scheduler.as_mut().unwrap();

    if let Some(fd) = scheduler.get_fd(scheduler.current_pid(), arg.fd) {
        let mut fd = fd.borrow_mut();
        if fd.can_write() {
            Ok(fd.write(arg.buf))
        } else {
            block(arg.fd, fd::AccessType::Write)
        }
    } else {
        Ok(Err(WriteError::BadFd))
    }
}

fn close(_frame: &mut interrupt::InterruptFrame, fd: Fd) {
    let mut scheduler = scheduler::SCHEDULER.take().unwrap();
    let scheduler = scheduler.as_mut().unwrap();
    scheduler.set_fd(scheduler.current_pid(), fd, None);
}

fn pipe() -> (Fd, Fd) {
    let mut scheduler = scheduler::SCHEDULER.take().unwrap();
    let scheduler = scheduler.as_mut().unwrap();
    let (read, write) = fd::pipe();
    let pid = scheduler.current_pid();
    (
        scheduler.new_fd(pid, Rc::new(RefCell::new(read))),
        scheduler.new_fd(pid, Rc::new(RefCell::new(write))),
    )
}

fn fork(frame: &mut interrupt::InterruptFrame) -> Pid {
    // Write PID 0 into the result buffer, so it'll get returned to the child
    unsafe { *(frame.ecx as *mut Pid) = 0 };

    let mut scheduler = scheduler::SCHEDULER.take().unwrap();
    let scheduler = scheduler.as_mut().unwrap();
    scheduler.fork(frame)
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
impl Arg for u32 {
    unsafe fn validate(_arg: *const Self) -> bool {
        // Every u32 is valid
        true
    }
}
impl Arg for usize {
    unsafe fn validate(_arg: *const Self) -> bool {
        // Every usize is valid
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
impl<'a> Arg for ReadArg<'a> {
    unsafe fn validate(arg: *const Self) -> bool {
        <&mut [u8]>::validate(core::ptr::addr_of!((*arg).buf))
    }
}
impl<'a> Arg for WriteArg<'a> {
    unsafe fn validate(arg: *const Self) -> bool {
        <&[u8]>::validate(core::ptr::addr_of!((*arg).buf))
    }
}

struct Block {
    fd: Fd,
    access_type: fd::AccessType,
}
type Blocking<T> = Result<T, Block>;
fn block<T>(fd: Fd, access_type: fd::AccessType) -> Blocking<T> {
    Err(Block { fd, access_type })
}

/// If the syscall ID passed by the user process in `frame` matches `id`, decodes and validates the
/// arguments and invokes the syscall handler.
fn match_syscall<A: Arg, T, F: FnOnce(&mut interrupt::InterruptFrame, A) -> T>(
    frame: &mut interrupt::InterruptFrame,
    id: SyscallId,
    func: F,
) -> bool {
    // a nonblocking syscall is just a blocking syscall that doesn't block
    match_syscall_blocking(frame, id, |f, a| Ok(func(f, a)))
}

/// If the syscall ID passed by the user process in `frame` matches `id`, decodes and validates the
/// arguments and invokes the syscall handler.
/// The syscall handler may return an error to indicate that the operation is blocked on a file
/// descriptor. If this happens, the process will not be scheduled again until the file descriptor
/// is ready, at which point the syscall will be re-invoked.
fn match_syscall_blocking<
    A: Arg,
    T,
    F: FnOnce(&mut interrupt::InterruptFrame, A) -> Blocking<T>,
>(
    frame: &mut interrupt::InterruptFrame,
    id: SyscallId,
    func: F,
) -> bool {
    // Does the syscall number match?
    if frame.eax as u8 == id as u8 {
        // Get the buffers for storing the arguments and the results
        let arg_ptr = frame.ebx as *const A;
        let result_ptr = frame.ecx as *mut T;
        unsafe {
            // Ensure the argument is valid, and the result points to valid memory
            assert!(A::validate(arg_ptr), "invalid syscall arg");
            assert!(
                validate_ptr(result_ptr, true),
                "invalid syscall result buffer"
            );

            // Invoke the syscall with the argument
            let result = func(frame, arg_ptr.read());
            match result {
                // Did it block?
                Ok(r) => result_ptr.write(r), // No, store the result in memory and return.
                Err(block) => {
                    // Yes, schedule a new process.
                    let continuation = {
                        let mut scheduler = scheduler::SCHEDULER.take().unwrap();
                        let scheduler = scheduler.as_mut().unwrap();
                        scheduler.block(
                            scheduler.current_pid(),
                            block.fd,
                            block.access_type,
                            syscall,
                        );
                        scheduler.schedule(frame)
                    };

                    continuation(frame);
                }
            }
        }
        true
    } else {
        false
    }
}
