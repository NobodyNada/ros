//! A simple driver for reading data from a hard disk using PIO.
//! https://wiki.osdev.org/ATA_PIO_Mode
#![allow(clippy::identity_op)]

use core::convert::TryInto;

use crate::{util::Global, x86::io};
use alloc::vec::Vec;
use modular_bitfield::prelude::*;

use super::IoRwConvertible;

pub const SECTOR_SIZE: usize = 512;
pub const PIO_BASE: u16 = 0x1f0;

#[derive(Default)]
pub struct Pio<const BASE: u16> {
    data: io::Io<u16, BASE, 0>,
    error: io::Input<Error, BASE, 1>,
    features: io::Output<u8, BASE, 1>,
    sector_count: io::Io<u8, BASE, 2>,
    lba_low: io::Io<u8, BASE, 3>,
    lba_mid: io::Io<u8, BASE, 4>,
    lba_hi: io::Io<u8, BASE, 5>,
    drive_head: io::Io<DriveHead, BASE, 6>,
    status: io::Input<Status, BASE, 7>,
    command: io::Output<u8, BASE, 7>,
}
pub static PIO: Global<Pio<PIO_BASE>> = Global::lazy_default();

#[bitfield]
#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub struct Error {
    /// Address mark not found.
    pub amnf: bool,
    /// Track zero not found.
    pub tkznf: bool,
    /// Aborted command.
    pub abrt: bool,
    /// Media change request.
    pub mcr: bool,
    /// ID not found.
    pub idnf: bool,
    /// Media changed.
    pub mc: bool,
    /// Uncorrectable data.
    pub unc: bool,
    /// Bad block.
    pub bbk: bool,
}
impl IoRwConvertible for Error {
    type Io = u8;
}

#[bitfield]
#[repr(u8)]
#[derive(Copy, Clone)]
struct DriveHead {
    lba_top: B4,
    drv: bool,

    #[skip]
    __: B1,

    lba: bool,

    #[skip]
    __: B1,
}
impl IoRwConvertible for DriveHead {
    type Io = u8;
}

#[bitfield]
#[repr(u8)]
#[derive(Debug, Copy, Clone)]
struct Status {
    /// Error
    err: bool,
    /// Index
    idx: bool,
    /// Corrected data
    corr: bool,
    /// Data ready
    drq: bool,
    /// Overlapped Mode Service Request
    srv: bool,
    /// Drive fault
    df: bool,
    /// Drive is active
    rdy: bool,
    /// Busy
    bsy: bool,
}
impl IoRwConvertible for Status {
    type Io = u8;
}

#[repr(u8)]
enum Command {
    Read = 0x20,
}

impl<const BASE: u16> Pio<BASE> {
    fn wait(&mut self) -> Result<(), Error> {
        unsafe {
            // Wait about 400ns to allow the status register to settle
            for _ in 0..15 {
                self.status.read();
            }
            loop {
                let status = self.status.read();
                if status.err() || status.df() {
                    return Err(self.error.read());
                } else if status.bsy() {
                    continue;
                } else if status.rdy() {
                    return Ok(());
                }
            }
        }
    }

    pub fn read(&mut self, mut buf: &mut [u8], idx: u32, count: u8) -> Result<(), Error> {
        assert_eq!(
            buf.len(),
            SECTOR_SIZE * count as usize,
            "read buffer size mismatch"
        );

        self.wait()?;

        if count == 0 {
            // a count of 0 tells the drive to read 32MB, which is not what we want
            return Ok(());
        }

        unsafe {
            self.drive_head.write(
                DriveHead::new()
                    .with_lba_top((idx >> 24) as u8)
                    .with_lba(true),
            );
            self.sector_count.write(count);
            self.lba_low.write(idx as u8);
            self.lba_mid.write((idx >> 8) as u8);
            self.lba_hi.write((idx >> 16) as u8);
            self.command.write(Command::Read as u8);

            while !buf.is_empty() {
                // read a sector from the drive
                self.wait()?;

                let word_buf =
                    core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u16, SECTOR_SIZE / 2);
                word_buf.iter_mut().for_each(|w| *w = self.data.read());

                // advance the buffer pointer to the next sector
                buf = &mut buf[SECTOR_SIZE..];
            }
        }
        Ok(())
    }

    pub fn reader(&mut self, start_sector: u32) -> PioReader<'_, BASE> {
        PioReader::new(self, start_sector)
    }
}

pub struct PioReader<'a, const BASE: u16> {
    pio: &'a mut Pio<BASE>,
    buffer: Vec<u8>,
    error: Option<Error>,
    buf_idx: usize,
    sector_idx: u32,
}

impl<'a, const BASE: u16> PioReader<'a, BASE> {
    pub fn new(pio: &'a mut Pio<BASE>, start_sector: u32) -> Self {
        PioReader {
            pio,
            buffer: Vec::new(),
            error: None,
            buf_idx: 0,
            sector_idx: start_sector,
        }
    }

    pub fn prefetch(&mut self, sectors: usize) -> Result<(), Error> {
        if self.buf_idx == self.buffer.len() {
            self.buf_idx = 0;
            self.buffer.clear();
        }

        let sectors = sectors.try_into().unwrap_or(u8::MAX);

        let start = self.buffer.len();
        self.buffer
            .resize(start + (SECTOR_SIZE * sectors as usize), 0);
        self.pio
            .read(&mut self.buffer[start..], self.sector_idx, sectors)?;
        self.sector_idx += sectors as u32;
        Ok(())
    }
}

impl<'a, const BASE: u16> Iterator for PioReader<'a, BASE> {
    type Item = Result<u8, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        let result = if let Some(error) = self.error {
            Err(error)
        } else if self.buf_idx < self.buffer.len() {
            let result = self.buffer[self.buf_idx];
            self.buf_idx += 1;
            Ok(result)
        } else {
            match self.prefetch(1) {
                Ok(()) => {
                    let result = self.buffer[self.buf_idx];
                    self.buf_idx += 1;
                    Ok(result)
                }
                Err(e) => Err(e),
            }
        };

        Some(result)
    }
}
