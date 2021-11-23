//! Some simple syscalls to test userspace
use crate::{
    kprintln, scheduler,
    x86::{interrupt, mmu},
};

pub extern "C" fn exit(frame: &mut interrupt::InterruptFrame) {
    let mut scheduler = scheduler::SCHEDULER.take().unwrap();
    let scheduler = scheduler.as_mut().unwrap();

    kprintln!("Process {} exited.", scheduler.current_pid());
    scheduler.kill_current_process(frame);
}

pub extern "C" fn yield_cpu(frame: &mut interrupt::InterruptFrame) {
    let mut scheduler = scheduler::SCHEDULER.take().unwrap();
    let scheduler = scheduler.as_mut().unwrap();

    scheduler.schedule(frame);
}

pub extern "C" fn puts(frame: &mut interrupt::InterruptFrame) {
    let (vaddr, size) = (frame.eax, frame.ebx);

    let mmu = mmu::MMU.take().unwrap();
    let mmu = &*mmu;

    let mut scheduler = scheduler::SCHEDULER.take().unwrap();
    let scheduler = scheduler.as_mut().unwrap();

    let pid = scheduler.current_pid();

    if !mmu.mapper.validate_range(
        &mmu.allocator,
        vaddr,
        size,
        mmu::mmap::MappingFlags::new().with_user_accessible(true),
    ) {
        let mut scheduler = scheduler::SCHEDULER.take().unwrap();
        let scheduler = scheduler.as_mut().unwrap();

        kprintln!("Killing process {} (invalid memory access)", pid);
        scheduler.kill_current_process(frame);
    }

    let buf = unsafe { core::slice::from_raw_parts(vaddr as *const _, size) };
    match core::str::from_utf8(buf) {
        Ok(text) => kprintln!("[{}]: {}", pid, text),
        Err(err) => kprintln!("[{}]: <invalid UTF-8: {:?}>", pid, err),
    }
}
