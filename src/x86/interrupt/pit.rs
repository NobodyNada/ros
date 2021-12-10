#![allow(clippy::identity_op)]
use crate::{
    util::Global,
    x86::{
        interrupt,
        io::{Io, IoRwConvertible, Output},
    },
};
use modular_bitfield::prelude::*;

const PIT_BASE: u16 = 0x40;

/// The programmable interrupt timer, which allows us to
/// set up a clock interrupt for preemptive multitasking.
pub struct Pit {
    channel_0: Io<u8, PIT_BASE, 0>,
    channel_1: Io<u8, PIT_BASE, 1>,
    channel_2: Io<u8, PIT_BASE, 2>,
    command: Output<Command, PIT_BASE, 3>,
}
pub static PIT: Global<Pit> = Global::lazy(|| unsafe { Pit::new() });

impl Pit {
    /// PIT interrupt number
    pub const IRQ: usize = 0;

    /// Sample rate input (before the divisor)
    pub const RATE: u32 = 1193182;

    unsafe fn new() -> Pit {
        let mut pit = Pit {
            channel_0: Default::default(),
            channel_1: Default::default(),
            channel_2: Default::default(),
            command: Default::default(),
        };
        pit.set_divisor(u16::MAX);
        pit
    }

    pub fn set_divisor(&mut self, divisor: u16) {
        unsafe {
            self.command.write(
                Command::new()
                    .with_mode(2) // rate generator
                    .with_access_mode(3), // write lo/hi
            );

            self.channel_0.write(divisor as u8);
            self.channel_0.write((divisor >> 8) as u8);
        }
    }

    pub fn handle_interrupt(frame: &mut interrupt::InterruptFrame) {
        interrupt::pic::Pic::eoi(Self::IRQ);
        crate::process::scheduler::Scheduler::handle_interrupt(frame);
    }
}

#[bitfield]
#[repr(u8)]
#[derive(Clone, Copy)]
struct Command {
    bcd: bool,
    mode: B3,
    access_mode: B2,
    channel: B2,
}
impl IoRwConvertible for Command {
    type Io = u8;
}
