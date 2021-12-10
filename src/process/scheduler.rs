use crate::{
    process::fd,
    syscall::Fd,
    util::Global,
    x86::{self, env::Env, interrupt::InterruptFrame},
};
use alloc::rc::Rc;
use core::{cell::RefCell, ops::DerefMut};
use hashbrown::HashMap;

pub type Pid = u32;

pub struct Scheduler {
    processes: HashMap<Pid, Process>,
    first: Option<Pid>,
    next: Option<Pid>,
    current_process: Pid,
    next_pid: Pid,
}

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
    fd: Fd,
    access_type: fd::AccessType,
    continuation: fn(&mut InterruptFrame),
}

impl Scheduler {
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
                let process = self.processes.get_mut(&pid).unwrap();
                let mut continuation: fn(&mut InterruptFrame) = |_| {};

                if let Some(block) = process.block.as_ref() {
                    let file = &process.fdtable[&block.fd];
                    if file.borrow_mut().can_access(block.access_type) {
                        // The process is no longer blocked, schedule it now.
                        continuation = block.continuation;
                        process.block = None;
                    } else {
                        // This process is still blocked, try the next one.
                        continue;
                    }
                }

                self.current_process = pid;
                unsafe {
                    x86::mmu::MMU
                        .take()
                        .unwrap()
                        .mapper
                        .set_cr3(process.env.cr3);
                }
                trap_frame.clone_from(&process.env.trap_frame);

                self.next = process.next;
                if self.next.is_none() {
                    // We've gone through the whole list of processes, go back to the start.
                    self.next = self.first;

                    // In case we're spinning for a long time waiting for something to do,
                    // run kernel tasks at the beginning of each round.
                    self.run_kernel_tasks();
                }

                // We've found a process; we're done.
                break continuation;
            } else {
                panic!("all processes exited");
            }
        }
    }

    pub fn current_pid(&self) -> Pid {
        self.current_process
    }

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

    pub fn get_fd(&self, pid: Pid, fd: Fd) -> Option<&Rc<RefCell<dyn fd::File>>> {
        self.processes
            .get(&pid)
            .and_then(|process| process.fdtable.get(&fd))
    }

    pub fn set_fd(&mut self, pid: Pid, fd: Fd, file: Option<Rc<RefCell<dyn fd::File>>>) {
        let process = self.processes.get_mut(&pid).expect("invalid process");
        if let Some(file) = file {
            process.fdtable.insert(fd, file);
        } else {
            process.fdtable.remove(&fd);
        }
        process.next_fd = core::cmp::max(process.next_fd, fd + 1);
    }

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
    pub fn block(
        &mut self,
        pid: Pid,
        fd: Fd,
        access_type: fd::AccessType,
        continuation: fn(&mut InterruptFrame),
    ) {
        self.processes.get_mut(&pid).expect("invalid process").block = Some(Block {
            fd,
            access_type,
            continuation,
        });
    }
}
