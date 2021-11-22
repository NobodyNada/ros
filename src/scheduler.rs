use crate::{
    util::Global,
    x86::{self, env::Env},
};
use hashbrown::HashMap;

pub type Pid = u32;

pub struct Scheduler {
    processes: HashMap<Pid, Process>,
    first_runnable: Option<Pid>,
    next_runnable: Option<Pid>,
    current_process: Pid,
    next_pid: Pid,
}

pub static SCHEDULER: Global<Option<Scheduler>> = Global::new(None);

struct Process {
    env: Env,
    next_runnable: Option<Pid>,
    prev_runnable: Option<Pid>,
}

impl Scheduler {
    pub fn new(init_process: Env) -> Scheduler {
        let mut processes = HashMap::new();
        processes.insert(
            1,
            Process {
                env: init_process,
                next_runnable: None,
                prev_runnable: None,
            },
        );

        Scheduler {
            processes,
            first_runnable: Some(1),
            next_runnable: Some(1),
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
        self.save_current_process(trap_frame);
        self.load_next_process(trap_frame);
    }

    fn save_current_process(&mut self, trap_frame: &x86::interrupt::InterruptFrame) {
        let process = self.processes.get_mut(&self.current_process).unwrap();
        process.env.trap_frame.clone_from(trap_frame);
    }

    fn load_next_process(&mut self, trap_frame: &mut x86::interrupt::InterruptFrame) {
        if let Some(pid) = self.next_runnable {
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

            self.next_runnable = process.next_runnable.or(self.first_runnable);
        } else {
            todo!("no runnable processes, implement an idle task");
        }
    }

    pub fn current_pid(&self) -> Pid {
        self.current_process
    }

    pub fn add_process(&mut self, env: Env) -> Pid {
        let new_pid = self.next_pid;
        self.next_pid += 1;

        self.processes.insert(
            self.next_pid,
            Process {
                env,
                next_runnable: self.first_runnable,
                prev_runnable: None,
            },
        );

        if let Some(process) = self.first_runnable {
            self.processes.get_mut(&process).unwrap().prev_runnable = Some(new_pid);
        }
        self.first_runnable = Some(new_pid);

        new_pid
    }

    pub fn remove_process(&mut self, pid: Pid) -> Env {
        let process = self.processes.remove(&pid).unwrap();

        if self.first_runnable == Some(pid) {
            self.first_runnable = process.next_runnable;
        }
        if self.next_runnable == Some(pid) {
            self.next_runnable = process.next_runnable.or(self.first_runnable);
        }
        if let Some(next) = process.next_runnable {
            self.processes.get_mut(&next).unwrap().prev_runnable = process.prev_runnable
        }
        if let Some(prev) = process.prev_runnable {
            self.processes.get_mut(&prev).unwrap().next_runnable = process.next_runnable
        }

        process.env
    }
}
