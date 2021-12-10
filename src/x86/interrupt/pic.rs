#![allow(clippy::identity_op)]
use crate::util::Global;
use crate::x86::{
    interrupt,
    io::{Io, Output},
};

const PIC_MASTER: u16 = 0x20;
const PIC_SLAVE: u16 = 0xA0;

pub struct Pic {
    master: PicIo<PIC_MASTER>,
    slave: PicIo<PIC_SLAVE>,
    mask: u16,
}
pub static PIC: Global<Pic> = Global::lazy(|| unsafe { Pic::new() });

impl Pic {
    pub const IRQ_SPURIOUS_MASTER: usize = 7;
    pub const IRQ_SPURIOUS_SLAVE: usize = 15;

    unsafe fn new() -> Pic {
        let mut pic = Pic {
            master: Default::default(),
            slave: Default::default(),
            mask: 0xffff,
        };
        pic.reset();
        pic
    }

    pub fn reset(&mut self) {
        const IRQ_SLAVE: u8 = 2;
        self.mask = 0xffff;
        unsafe {
            self.master
                .initialize(interrupt::IRQ_OFFSET as u8, 1 << IRQ_SLAVE, 0x3);
            self.slave
                .initialize(interrupt::IRQ_OFFSET as u8 + 8, IRQ_SLAVE, 0x1);
        }
        self.unmask(2); // unmask slave interrupt
                        // acknowledge any pending interrupts
        Self::eoi(Self::IRQ_SPURIOUS_SLAVE);
    }

    fn write_mask(&mut self) {
        unsafe {
            crate::kprintln!("write mask {:#x}", self.mask);
            self.master.data.write(self.mask as u8);
            self.slave.data.write((self.mask >> 8) as u8);
        }
    }

    pub fn set_mask(&mut self, mask: u16) {
        self.mask = mask;
        self.write_mask();
    }

    pub fn mask(&mut self, interrupt: usize) {
        assert!(interrupt < 16, "interrupt out of range");
        self.mask |= 1 << interrupt;
        self.write_mask();
    }

    pub fn unmask(&mut self, interrupt: usize) {
        assert!(interrupt < 16, "interrupt out of range");
        self.mask &= !(1 << interrupt);
        self.write_mask();
    }

    pub fn eoi(interrupt: usize) {
        assert!(interrupt < 16, "interrupt out of range");
        unsafe {
            PicIo::<PIC_MASTER>::default().command.write(0x20);
            if interrupt > 7 {
                PicIo::<PIC_SLAVE>::default().command.write(0x20);
            }
        }
    }

    pub fn handle_spurious_master(_frame: interrupt::InterruptFrame) {
        // ignore
    }

    pub fn handle_spurious_slave(_frame: interrupt::InterruptFrame) {
        // acknowledge the spurious interrupt on the master only
        Self::eoi(Self::IRQ_SPURIOUS_MASTER);
    }
}

#[derive(Default)]
struct PicIo<const BASE: u16> {
    command: Output<u8, BASE, 0>,
    data: Io<u8, BASE, 1>,
}

impl<const BASE: u16> PicIo<BASE> {
    unsafe fn initialize(&mut self, vector_offset: u8, slave_mask: u8, icw4: u8) {
        // mask all interrupts
        self.data.write(0xFF);

        // send initialization command
        self.command.write(0x11); // edge triggering, cascaded, with ICW4
        self.data.write(vector_offset);
        self.data.write(slave_mask);
        self.data.write(icw4);
    }
}
