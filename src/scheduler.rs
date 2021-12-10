use crate::{
    fd,
    syscall::Fd,
    util::Global,
    x86::{self, env::Env},
};
use alloc::rc::Rc;
use core::cell::RefCell;
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

        let mut trap_frame = x86::interrupt::InterruptFrame::default();
        self.load_next_process(&mut trap_frame);

        *global_scheduler_ref = Some(self);
        core::mem::drop(global_scheduler_ref);

        tss.ss0 = x86::mmu::SegmentId::KernelData as u16;
        unsafe {
            call_user(core::ptr::addr_of_mut!(tss.esp0), trap_frame);
        }
        #[naked]
        unsafe extern "C" fn call_user(
            esp0: *mut u32,
            trap_frame: x86::interrupt::InterruptFrame,
        ) -> ! {
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

    pub fn schedule(&mut self, trap_frame: &mut x86::interrupt::InterruptFrame) {
        // Run high-priority kernel tasks first
        fd::CONSOLE_BUFFER.handle_echo();

        self.save_current_process(trap_frame);
        self.load_next_process(trap_frame);
    }

    fn save_current_process(&mut self, trap_frame: &x86::interrupt::InterruptFrame) {
        let process = self.processes.get_mut(&self.current_process).unwrap();
        process.env.trap_frame.clone_from(trap_frame);
    }

    fn load_next_process(&mut self, trap_frame: &mut x86::interrupt::InterruptFrame) {
        if let Some(pid) = self.next {
            self.current_process = pid;
            let process = &self.processes[&pid];
            unsafe {
                x86::mmu::MMU
                    .take()
                    .unwrap()
                    .mapper
                    .set_cr3(process.env.cr3);
            }
            trap_frame.clone_from(&process.env.trap_frame);

            self.next = process.next.or(self.first);
        } else {
            panic!("no runnable processes");
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

    pub fn kill_current_process(&mut self, trap_frame: &mut x86::interrupt::InterruptFrame) -> Env {
        let env = self.remove_process(self.current_pid());
        self.load_next_process(trap_frame);
        env
    }

    pub fn get_fd(&self, pid: Pid, fd: Fd) -> Option<&Rc<RefCell<dyn fd::File>>> {
        self.processes
            .get(&pid)
            .and_then(|process| process.fdtable.get(&fd))
    }

    pub fn set_fd(&mut self, pid: Pid, fd: Fd, file: Rc<RefCell<dyn fd::File>>) {
        self.processes
            .get_mut(&pid)
            .expect("invalid process")
            .fdtable
            .insert(fd, file);
    }
}
