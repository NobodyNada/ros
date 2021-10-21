#![allow(clippy::identity_op)]

use core::fmt::Write;

use crate::util::Global;

use super::{Io, Output};
use modular_bitfield::prelude::*;

pub const CGA_WIDTH: usize = 80;
pub const CGA_HEIGHT: usize = 25;

pub const CGA_REG_BASE: u16 = 0x3D4;
pub const CGA_MEM_BASE: u32 = 0xF00B8000;

/// A simple CGA driver for displaying a console.
pub struct Cga {
    buf: &'static mut [Char],
    reg_index: Output<u8, { CGA_REG_BASE }, 0>,
    reg_data: Io<u8, { CGA_REG_BASE }, 1>,
    cursor_x: usize,
    cursor_y: usize,
}

pub static CGA: Global<Cga> = Global::lazy(|| unsafe { Cga::new() });

impl Cga {
    pub unsafe fn new() -> Self {
        let mut cga = Cga {
            buf: core::slice::from_raw_parts_mut(CGA_MEM_BASE as *mut _, CGA_WIDTH * CGA_HEIGHT),
            reg_index: Output::new(),
            reg_data: Io::new(),
            cursor_x: 0,
            cursor_y: 0,
        };
        cga.clear();
        cga
    }

    pub fn clear(&mut self) {
        self.buf.fill(Char::default())
    }

    pub fn set_char(&mut self, x: usize, y: usize, c: Char) {
        self.buf[Cga::idx(x, y)] = c
    }

    pub fn write_char(&mut self, c: Char) {
        match c.c() {
            b'\n' => {
                self.cursor_y += 1;
                self.cursor_x = 0;
            }
            b'\t' => self.cursor_x = (self.cursor_x + 4) & !3,
            _ => {
                self.set_char(self.cursor_x, self.cursor_y, c);
                self.cursor_x += 1;
            }
        }

        if self.cursor_x >= CGA_WIDTH {
            self.cursor_x = 0;
            self.cursor_y += 1;
        }
        while self.cursor_y >= CGA_HEIGHT {
            // scroll the screen down a line
            self.cursor_y -= 1;
            self.buf
                .copy_within((1 * CGA_WIDTH)..(CGA_HEIGHT * CGA_WIDTH), 0);
            self.buf[((CGA_HEIGHT - 1) * CGA_WIDTH)..].fill(Char::default());
        }

        // Move the cursor on the screen
        let cursor = Self::idx(self.cursor_x, self.cursor_y) as u16;

        unsafe {
            self.reg_index.write(CgaReg::CursorPosHigh as u8);
            self.reg_data.write((cursor >> 8) as u8);
            self.reg_index.write(CgaReg::CursorPosLow as u8);
            self.reg_data.write(cursor as u8);
        }
    }

    pub fn write_byte(&mut self, c: u8) {
        self.write_char(Char::new().with_c(c).with_fg_color(Color::LightGray))
    }

    pub fn write_bytes(&mut self, s: &[u8]) {
        for c in s {
            self.write_byte(*c);
        }
    }

    fn idx(x: usize, y: usize) -> usize {
        assert!(
            x < CGA_WIDTH && y < CGA_HEIGHT,
            "character ({}, {}) out of bounds",
            x,
            y
        );
        y * CGA_WIDTH + x
    }
}

impl Write for Cga {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_bytes(s.as_bytes());
        Ok(())
    }
}

#[bitfield]
#[repr(u16)]
#[derive(Clone, Copy)]
pub struct Char {
    pub c: u8,
    pub fg_color: Color,
    pub bg_color: Color,
}
impl Default for Char {
    fn default() -> Char {
        Char::new()
            .with_c(b' ')
            .with_fg_color(Color::LightGray)
            .with_bg_color(Color::Black)
    }
}

#[derive(BitfieldSpecifier)]
#[bits = 4]
pub enum Color {
    Black = 0,
    Blue,
    Green,
    Cyan,
    Red,
    Magenta,
    Brown,
    LightGray,
    DarkGray,
    LightBlue,
    LightGreen,
    LightCyan,
    LightRed,
    LightMagenta,
    Yellow,
    White,
}

#[derive(Clone, Copy)]
enum CgaReg {
    CursorPosHigh = 0xe,
    CursorPosLow = 0xf,
}
