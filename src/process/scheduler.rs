use crate::{
    process::fd,
    syscall::Fd,
    util::Global,
    x86::{self, env::Env, interrupt::InterruptFrame},
};
use alloc::rc::Rc;
use core::{
    arch::asm,
    cell::RefCell,
    ops::DerefMut,
    sync::atomic::{AtomicBool, Ordering},
};
use hashbrown::HashMap;

pub type Pid = u32;

/// A simple round-robin scheduler.
pub struct Scheduler {
    processes: HashMap<Pid, Process>,
    first: Option<Pid>,
    next: Option<Pid>,
    current_process: Pid,
    next_pid: Pid,
}

/// The reason a process is blocked.
pub enum BlockReason {
    /// The process is waiting to access a file descriptor.
    File { fd: Fd, access_type: fd::AccessType },

    /// The process is waiting for another process to exit
    Process(Pid),
}

/// The global scheduler.
pub static SCHEDULER: Global<Option<Scheduler>> = Global::new(None);

struct Process {
    env: Env,
    next: Option<Pid>,
    prev: Option<Pid>,
    fdtable: HashMap<Fd, Rc<RefCell<dyn fd::File>>>,
    block: Option<Block>,
    next_fd: Fd,
}

struct Block {
    reason: BlockReason,
    continuation: fn(&mut InterruptFrame),
}

/// Set if the preemption timer fires during kernelspace.
static TIMER_FIRED: AtomicBool = AtomicBool::new(false);

impl Scheduler {
    /// The timeslice interval.
    pub const PREEMPT_RATE: u32 = 100; // 100 Hz

    /// Creates a new process using an initial environment.
    pub fn new(init_process: Env) -> Scheduler {
        let mut processes = HashMap::new();
        processes.insert(
            1,
            Process {
                env: init_process,
                next: None,
                prev: None,
                fdtable: HashMap::new(),
                block: None,
                next_fd: 0,
            },
        );

        Scheduler {
            processes,
            first: Some(1),
            next: Some(1),
            current_process: 1,
            next_pid: 2,
        }
    }

    /// Starts the scheduler.
    pub fn run(mut self) -> ! {
        let mut global_scheduler_ref = SCHEDULER.take().expect("a scheduler is already running");
        assert!(
            global_scheduler_ref.is_none(),
            "a scheduler is already running"
        );

        // Create TSS
        let mut tss = x86::env::TaskStateSegment::default();
        *x86::mmu::MMU.take().unwrap().gdt.last_mut().unwrap() =
            x86::mmu::segment::SegmentDescriptor::new()
                .with_segment_type(0b1001)
                .with_base(core::ptr::addr_of!(tss) as usize)
                .with_limit(core::mem::size_of_val(&tss) - 1)
                .with_present(true);
        unsafe {
            asm!("ltr {:x}", in(reg) x86::mmu::SegmentId::TaskState as u16, options(nomem, nostack))
        }

        let mut trap_frame = InterruptFrame::default();

        // We can safely ignore the continuation because we know the first process is not blocked.
        let _ = self.load_next_process(&mut trap_frame);

        *global_scheduler_ref = Some(self);
        core::mem::drop(global_scheduler_ref);

        tss.ss0 = x86::mmu::SegmentId::KernelData as u16;

        // Set up the preemption timer for 100 Hz
        x86::interrupt::pit::PIT
            .take()
            .unwrap()
            .set_divisor((x86::interrupt::pit::Pit::RATE / Self::PREEMPT_RATE) as u16);
        x86::interrupt::pic::PIC
            .take()
            .unwrap()
            .unmask(x86::interrupt::pit::Pit::IRQ);
        unsafe {
            call_user(core::ptr::addr_of_mut!(tss.esp0), trap_frame);
        }
        #[naked]
        unsafe extern "C" fn call_user(esp0: *mut u32, trap_frame: InterruptFrame) -> ! {
            asm!(
                "add esp, 4", // pop return adddress
                "pop eax",    // save kernel stack pointer in TSS
                "mov [eax], esp",
                "pop eax", // pop segments
                "mov ds, ax",
                "pop eax",
                "mov es, ax",
                "pop eax",
                "mov fs, ax",
                "pop eax",
                "mov gs, ax",
                "popad",      // pop registers
                "add esp, 8", // pop interrupt ID and code
                "iretd",      // jump into userspace
                options(noreturn)
            );
        }
    }

    /// Schedules a process. Also returns a continuation function that must be invoked immediately
    /// before returning to userspace, since the process may have been scheduled out during an
    /// interruptible kernel function that needs to complete.
    ///
    /// The continuation must not be invoked while the caller is holding a reference to the
    /// scheduler or any other kernel resources.
    #[must_use]
    pub fn schedule(&mut self, trap_frame: &mut InterruptFrame) -> fn(&mut InterruptFrame) {
        // Run high-priority kernel tasks first
        self.run_kernel_tasks();
        self.save_current_process(trap_frame);
        self.load_next_process(trap_frame)
    }

    /// Forks the current process, returning the child's PID.
    /// The MMU environment and all file descriptors are copied.
    pub fn fork(&mut self, trap_frame: &InterruptFrame) -> Pid {
        let new_cr3 = {
            let mut mmu = x86::mmu::MMU.take().unwrap();
            let mmu = mmu.deref_mut();
            mmu.mapper.fork(&mut mmu.allocator)
        };

        // Use the new MMU env for the old process, because that requires one less MMU switch.
        let current_process = self.processes.get_mut(&self.current_pid()).unwrap();
        let new_cr3 = core::mem::replace(&mut current_process.env.cr3, new_cr3);
        let new_fdtable = current_process.fdtable.clone();
        assert!(
            current_process.block.is_none(),
            "cannot fork a blocked process"
        );

        let new_pid = self.add_process(Env {
            trap_frame: trap_frame.clone(),
            cr3: new_cr3,
        });
        // Copy file descriptors
        self.processes.get_mut(&new_pid).unwrap().fdtable = new_fdtable;

        new_pid
    }

    fn run_kernel_tasks(&mut self) {
        fd::CONSOLE_BUFFER.handle_echo();
    }

    fn save_current_process(&mut self, trap_frame: &InterruptFrame) {
        let process = self.processes.get_mut(&self.current_process).unwrap();
        process.env.trap_frame.clone_from(trap_frame);
    }

    #[must_use]
    fn load_next_process(&mut self, trap_frame: &mut InterruptFrame) -> fn(&mut InterruptFrame) {
        loop {
            if let Some(pid) = self.next {
                let process = self.processes.get(&pid).unwrap();
                let mut continuation: fn(&mut InterruptFrame) = |_| {};

                // Is this process blocked?
                let blocked = if let Some(block) = process.block.as_ref() {
                    if self.can_unblock(process, &block.reason) {
                        // The process is no longer blocked.
                        continuation = block.continuation;
                        false
                    } else {
                        true
                    }
                } else {
                    false
                };

                self.next = process.next;
                if self.next.is_none() {
                    // We've gone through the whole list of processes, go back to the start.
                    self.next = self.first;

                    // In case we're spinning for a long time waiting for something to do,
                    // run kernel tasks at the beginning of each round.
                    self.run_kernel_tasks();
                }

                if !blocked {
                    // This process is not blcoed; schedule it now.
                    let process = self.processes.get_mut(&pid).unwrap();
                    process.block = None;

                    self.current_process = pid;
                    unsafe {
                        x86::mmu::MMU
                            .take()
                            .unwrap()
                            .mapper
                            .set_cr3(process.env.cr3);
                    }
                    trap_frame.clone_from(&process.env.trap_frame);

                    // We've found a process; we're done.
                    break continuation;
                }
            } else {
                panic!("all processes exited");
            }
        }
    }

    fn can_unblock(&self, process: &Process, reason: &BlockReason) -> bool {
        match reason {
            BlockReason::File { fd, access_type } => {
                process.fdtable[fd].borrow_mut().can_access(*access_type)
            }
            BlockReason::Process(pid) => !self.processes.contains_key(pid),
        }
    }

    /// Returns the process ID of the currently executing process.
    pub fn current_pid(&self) -> Pid {
        self.current_process
    }

    /// Adds a new process to the scheduler.
    pub fn add_process(&mut self, env: Env) -> Pid {
        let new_pid = self.next_pid;
        self.next_pid += 1;

        self.processes.insert(
            new_pid,
            Process {
                env,
                next: self.first,
                prev: None,
                fdtable: HashMap::new(),
                block: None,
                next_fd: 0,
            },
        );

        if let Some(process) = self.first {
            self.processes.get_mut(&process).unwrap().prev = Some(new_pid);
        }
        self.first = Some(new_pid);

        new_pid
    }

    /// Removes a process from the scheduler.
    pub fn remove_process(&mut self, pid: Pid) -> Env {
        let process = self.processes.remove(&pid).unwrap();

        if self.first == Some(pid) {
            self.first = process.next;
        }
        if self.next == Some(pid) {
            self.next = process.next.or(self.first);
        }
        if let Some(next) = process.next {
            self.processes.get_mut(&next).unwrap().prev = process.prev
        }
        if let Some(prev) = process.prev {
            self.processes.get_mut(&prev).unwrap().next = process.next
        }

        process.env
    }

    /// Returns true if the specified PID corresponds to a running process.
    pub fn process_exists(&self, pid: Pid) -> bool {
        self.processes.contains_key(&pid)
    }

    /// Terminates the current process and schedules a new process in its place.
    ///
    /// Returns the MMU environment of the old process, and a continuation function that must be
    /// invoked before returning to userspace (see the documentation for `schedule`).
    #[must_use]
    pub fn kill_current_process(
        &mut self,
        trap_frame: &mut InterruptFrame,
    ) -> (Env, fn(&mut InterruptFrame)) {
        let env = self.remove_process(self.current_pid());
        (env, self.load_next_process(trap_frame))
    }

    /// Returns a reference to the file object for a given process and file descriptor.
    pub fn get_fd(&self, pid: Pid, fd: Fd) -> Option<&Rc<RefCell<dyn fd::File>>> {
        self.processes
            .get(&pid)
            .and_then(|process| process.fdtable.get(&fd))
    }

    /// Sets the file object for a given process and file descriptor.
    pub fn set_fd(&mut self, pid: Pid, fd: Fd, file: Option<Rc<RefCell<dyn fd::File>>>) {
        let process = self.processes.get_mut(&pid).expect("invalid process");
        if let Some(file) = file {
            process.fdtable.insert(fd, file);
        } else {
            process.fdtable.remove(&fd);
        }
        process.next_fd = core::cmp::max(process.next_fd, fd + 1);
    }

    /// Creates a new file descriptor for the given file.
    pub fn new_fd(&mut self, pid: Pid, file: Rc<RefCell<dyn fd::File>>) -> Fd {
        let process = self.processes.get_mut(&pid).expect("invalid process");
        let fd = process.next_fd;
        process.next_fd += 1;
        process.fdtable.insert(fd, file);
        fd
    }

    /// Blocks a process on the given file descriptor.
    /// The process will not be scheduled until the file descriptor is ready to access.
    /// The provided continuation is invoked once the process is un-blocked, before the process is
    /// scheduled for the first time, to allow the kernel to resume an interrupted syscall.
    pub fn block(&mut self, pid: Pid, reason: BlockReason, continuation: fn(&mut InterruptFrame)) {
        self.processes.get_mut(&pid).expect("invalid process").block = Some(Block {
            reason,
            continuation,
        });
    }

    /// Handles an incoming timer interrupt.
    pub fn handle_interrupt(frame: &mut InterruptFrame) {
        if frame.is_userspace() {
            TIMER_FIRED.store(false, Ordering::Relaxed);
            // // Preempt the user process.
            let continuation = SCHEDULER
                .take()
                .expect("scheduler conflict in userspace?")
                .as_mut()
                .expect("no scheduler in userspace?")
                .schedule(frame);
            continuation(frame);
        } else {
            TIMER_FIRED.store(true, Ordering::Relaxed);
        }
    }

    /// If the preemption timer fired while we were in kernelspace, schedules a new processs now.
    pub fn preempt_if_needed(&mut self, frame: &mut InterruptFrame) {
        if TIMER_FIRED.load(Ordering::Relaxed) {
            // Preempt the user process.
            TIMER_FIRED.store(false, Ordering::Relaxed);
            let continuation = self.schedule(frame);
            continuation(frame);
        }
    }
}
