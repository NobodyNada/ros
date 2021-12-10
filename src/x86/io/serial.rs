#![allow(clippy::identity_op)]

use core::fmt::Write;

use super::{Input, Io, IoRwConvertible, Output};
use crate::{util::Global, x86::interrupt};
use modular_bitfield::prelude::*;

/// A basic PC16550 serial driver.
pub struct Serial<const BASE: u16> {
    io: SerialIo<BASE>,
}

/// The default serial port.
pub static COM1: Global<Serial<COM1_BASE>> = Global::lazy(|| unsafe { Serial::new() });

/// The I/O device base address for the default serial port.
pub const COM1_BASE: u16 = 0x3F8;
pub const COM1_IRQ: usize = 4;

impl<const BASE: u16> Serial<BASE> {
    /// Instantiates and initializes a serial port.
    ///
    /// # Safety
    ///
    /// It is the caller's responsibility to avoid I/O space conflicts (such as two serial drivers
    /// sharing the same port).
    pub unsafe fn new() -> Self {
        let mut serial = Self {
            io: SerialIo::default(),
        };
        serial.reset();
        serial
    }

    /// Reinitializes the serial port.
    pub fn reset(&mut self) {
        unsafe {
            self.set_divisor_latch(false);

            // Disable all interrupts, clear and enable the FIFOs
            self.io.interrupt_enable.write(InterruptEnable::new());
            self.io.fifo_control.write(
                FifoControl::new()
                    .with_reset_tx(true)
                    .with_reset_rx(true)
                    .with_enabled(true),
            );

            self.set_baud_divisor((115200 / 9600) as u16);
        }
    }

    pub fn enable_interrupts(&mut self) {
        unsafe {
            interrupt::with_interrupts_disabled(|| {
                self.io
                    .interrupt_enable
                    .write(InterruptEnable::new().with_receiver_ready(true));
                interrupt::pic::PIC.take().unwrap().unmask(COM1_IRQ);

                // flush the buffer
                Self::recv();
            })
        }
    }

    /// Outputs a single byte over the serial port. Blocks if the transmit FIFO is full.
    pub fn write_byte(&mut self, val: u8) {
        unsafe {
            // Wait for space in the transmit FIFO
            while !self.io.line_status.read().transmitter_holding_empty() {}
            self.io.data_holding.write(val);
        }
    }

    /// Outputs some bytes over the serial port. Blocks if the transmit FIFO is full.
    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.write_byte(*b);
        }
    }

    /// Sets the baud rate divisor.
    ///
    /// A divisor of 1 corresponds to a baud rate of 115,200 bits per second.
    pub fn set_baud_divisor(&mut self, divisor: u16) {
        self.with_divisor_latch(|s| unsafe {
            s.io.divisor_lsb.write(divisor as u8);
            s.io.divisor_msb.write((divisor >> 8) as u8);
        })
    }

    /// Handles incoming serial data.
    ///
    /// # Safety
    ///
    /// This function can safely run at the same time as a serial transmission, however it is not
    /// safe to perform two recieves in parallel. The caller must ensure the reciever is unique, by
    /// calling 'recv' only with interrupts disabled.
    pub unsafe fn recv() {
        let mut io = SerialIo::<BASE>::default();
        while io.line_status.read().recieve_data_ready() {
            crate::process::fd::CONSOLE_BUFFER.recv_input(match io.data_holding.read() {
                b'\r' => b'\n', // replace carriage return with newline
                x => x,
            });
        }
    }

    /// Handles a serial port interrupt.
    ///
    /// # Safety
    ///
    /// This function is not thread-safe or reentrant. The caller must ensure the ISR is
    /// called only from an interrupt context.
    pub unsafe fn handle_interrupt(_frame: &mut interrupt::InterruptFrame) {
        Self::recv();
    }

    unsafe fn set_divisor_latch(&mut self, latch: bool) {
        self.io.line_control.write(
            LineControl::new()
                .with_word_length(0x3) // 8-bit words
                .with_divisor_latch(latch),
        );
    }

    fn with_divisor_latch<F: FnOnce(&mut Self) -> T, T>(&mut self, f: F) -> T {
        unsafe {
            interrupt::with_interrupts_disabled(|| {
                self.set_divisor_latch(true);
                let result = f(self);
                self.set_divisor_latch(false);
                result
            })
        }
    }
}

impl<const BASE: u16> Write for Serial<BASE> {
    /// Outputs a string over the serial port. Blocks if the transmit FIFO is full.
    /// Always succeeds.
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_bytes(s.as_bytes());
        Ok(())
    }
}

#[derive(Default)]
struct SerialIo<const BASE: u16> {
    // NOTE: only accessible when divisor latch is clear
    pub data_holding: Io<u8, BASE, 0>,
    pub interrupt_enable: Io<InterruptEnable, BASE, 0x1>,

    // NOTE: only accessible when divisor latch is set
    pub divisor_lsb: Output<u8, BASE, 0>,
    pub divisor_msb: Output<u8, BASE, 0>,

    pub interrupt_status: Input<InterruptStatus, BASE, 0x2>,
    pub fifo_control: Output<FifoControl, BASE, 0x2>,
    pub line_control: Output<LineControl, BASE, 0x3>,
    pub modem_control: Output<ModemControl, BASE, 0x4>,
    pub line_status: Input<LineStatus, BASE, 0x5>,
    pub modem_status: Input<ModemStatus, BASE, 0x6>,
    pub scratchpad: Io<u8, BASE, 0x7>,
}

#[bitfield]
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
struct InterruptEnable {
    receiver_ready: bool,
    transmit_complete: bool,
    receiver_line_status: bool,

    modem_line_status: bool,
    #[skip]
    __: B4,
}
impl IoRwConvertible for InterruptEnable {
    type Io = u8;
}

#[bitfield]
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
struct InterruptStatus {
    interrupt_id: InterruptId,

    #[skip]
    __: B4,
}
impl IoRwConvertible for InterruptStatus {
    type Io = u8;
}

#[derive(Debug, BitfieldSpecifier)]
#[bits = 4]
enum InterruptId {
    None = 1,
    LineStatus = 0x6,
    RxReady = 0x4,
    RxTimeout = 0xA,
    TxReady = 0x2,
    ModemStatus = 0,
}

#[bitfield]
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
struct FifoControl {
    enabled: bool,
    reset_rx: bool,
    reset_tx: bool,
    rdy_mode: bool,

    #[skip]
    __: B2,

    trigger_level: B2,
}
impl IoRwConvertible for FifoControl {
    type Io = u8;
}

#[bitfield]
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
struct LineControl {
    word_length: B2,
    stop_bits: bool,

    parity_enable: bool,
    even_parity: bool,
    force_parity: bool,

    set_break: bool,
    divisor_latch: bool,
}
impl IoRwConvertible for LineControl {
    type Io = u8;
}

#[bitfield]
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
struct ModemControl {
    dtr: bool,
    rts: bool,
    op1: bool,
    op2: bool,
    loopback: bool,

    #[skip]
    __: B3,
}
impl IoRwConvertible for ModemControl {
    type Io = u8;
}

#[bitfield]
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
struct LineStatus {
    recieve_data_ready: bool,

    buffer_overrun: bool,
    parity_error: bool,
    framing_error: bool,
    break_signal: bool,

    transmitter_holding_empty: bool,
    transitter_empty: bool,

    has_error: bool,
}
impl IoRwConvertible for LineStatus {
    type Io = u8;
}

#[bitfield]
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
struct ModemStatus {
    cts_changed: bool,
    dsr_changed: bool,
    ri_changed: bool,
    dcd_changed: bool,

    cts: bool,
    dsr: bool,
    ri: bool,
    dcd: bool,
}
impl IoRwConvertible for ModemStatus {
    type Io = u8;
}
