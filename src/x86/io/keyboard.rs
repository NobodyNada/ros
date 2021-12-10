#![allow(clippy::identity_op)]
use super::{Input, Io, IoRwConvertible, Output};
use crate::{
    util::Global,
    x86::interrupt::{self, InterruptFrame},
};
use modular_bitfield::prelude::*;

pub struct Keyboard {
    data: Io<u8, 0x60, 0>,
    status: Input<Status, 0x64, 0>,
    command: Output<u8, 0x64, 0>,
    left_shift: bool,
    right_shift: bool,
    escape: bool,
    release: bool,
}
pub static KEYBOARD: Global<Keyboard> = Global::lazy(|| unsafe { Keyboard::new() });

impl Keyboard {
    pub const IRQ: usize = 1;

    const DISABLE_PORT0: u8 = 0xAD;
    const ENABLE_PORT0: u8 = 0xAE;
    const DISABLE_PORT1: u8 = 0xA7;
    const READ_CONFIG: u8 = 0x20;
    const WRITE_CONFIG: u8 = 0x60;

    /// Instantiates and initializes a PS/2 keyboard.
    ///
    /// # Safety
    ///
    /// It is the caller's responsibility to avoid I/O space conflicts (such as two keyboard drivers
    /// sharing the same port).
    unsafe fn new() -> Self {
        let mut keyboard = Self::new_uninit();
        keyboard.reset();
        keyboard
    }

    unsafe fn new_uninit() -> Self {
        Self {
            data: Io::default(),
            status: Input::default(),
            command: Output::default(),
            left_shift: false,
            right_shift: false,
            escape: false,
            release: false,
        }
    }

    pub fn reset(&mut self) {
        unsafe {
            // disable the port
            self.command.write(Self::DISABLE_PORT0);
            self.command.write(Self::DISABLE_PORT1);

            // configure interupts
            self.command.write(Self::READ_CONFIG);
            while !self.status.read().output_ready() {}
            let old_config = Config::from(self.data.read());

            self.command.write(Self::WRITE_CONFIG);
            self.command.write(
                old_config
                    .with_interrupt_0(true)
                    .with_interrupt_1(false)
                    .into(),
            );

            // enable port0
            self.command.write(Self::ENABLE_PORT0);

            // unmask the interrupt in the interrupt controller
            interrupt::pic::PIC.take().unwrap().unmask(Self::IRQ);
        }
    }

    pub fn handle_interrupt(_frame: &mut InterruptFrame) {
        KEYBOARD.take().expect("keyboard conflict").handle_input();
    }
    pub fn handle_input(&mut self) {
        unsafe {
            loop {
                let status = self.status.read();
                if !status.output_ready() || status.mouse_or_transmission_error() {
                    break;
                }

                const LEFT_SHIFT: u8 = 0x12;
                const RIGHT_SHIFT: u8 = 0x12;
                const KEYMAP: [u8; 88] = [
                    0, 0x1B, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-',
                    b'=', 0, b'\t', b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p',
                    b'[', b']', b'\n', 0, b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l',
                    b';', b'\'', b'`', 0, b'\\', b'z', b'x', b'c', b'v', b'b', b'n', b'm', b',',
                    b'.', b'/', 0, b'*', 0, b' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, b'7',
                    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1', b'2', b'3', b'0', b'.', 0, 0,
                    0, 0,
                ];
                const SHIFTMAP: [u8; 88] = [
                    0, 0x1B, b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'_',
                    b'+', 0, b'\t', b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P',
                    b'{', b'}', b'\n', 0, b'A', b'S', b'D', b'F', b'G', b'H', b'J', b'K', b'L',
                    b':', b'"', b'~', 0, b'|', b'Z', b'X', b'C', b'V', b'B', b'N', b'M', b'<',
                    b'>', b'?', 0, b'*', 0, b' ', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, b'7',
                    b'8', b'9', b'-', b'4', b'5', b'6', b'+', b'1', b'2', b'3', b'0', b'.', 0, 0,
                    0, 0,
                ];
                let c = self.data.read();
                if self.release {
                    if !self.escape {
                        if c == LEFT_SHIFT {
                            self.left_shift = false;
                            continue;
                        } else if c == RIGHT_SHIFT {
                            self.right_shift = false;
                            continue;
                        }
                    }
                    self.release = false;
                    continue;
                }
                if c == 0xF0 {
                    self.release = true;
                    continue;
                }

                if self.escape {
                    self.escape = false;
                    continue;
                }
                if c == 0xE0 {
                    self.escape = true;
                    continue;
                }

                if c == LEFT_SHIFT {
                    self.left_shift = true;
                    continue;
                } else if c == RIGHT_SHIFT {
                    self.right_shift = true;
                    continue;
                }

                match KEYMAP.get(c as usize) {
                    None | Some(0) => continue,
                    Some(&c) => crate::fd::CONSOLE_BUFFER.recv_input(c),
                }
            }
        }
    }
}

#[bitfield]
#[repr(u8)]
#[derive(Clone, Copy)]
struct Status {
    pub output_ready: bool,
    pub input_ready: bool,
    pub system: bool,
    pub command: bool,

    pub keyboard_lock: bool,
    pub mouse_or_transmission_error: bool,
    pub timeout_error: bool,
    pub parity_error: bool,
}
impl IoRwConvertible for Status {
    type Io = u8;
}

#[bitfield]
#[repr(u8)]
#[derive(Clone, Copy)]
pub struct Config {
    interrupt_0: bool,
    interrupt_1: bool,
    system: bool,
    #[skip]
    __: bool,

    clock_disable_0: bool,
    clock_disable_1: bool,
    translation: bool,
    #[skip]
    __: bool,
}
impl IoRwConvertible for Config {
    type Io = u8;
}
