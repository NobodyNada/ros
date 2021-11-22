#![allow(dead_code, clippy::identity_op)]
use modular_bitfield::prelude::*;

#[bitfield]
pub struct SegmentDescriptor {
    limit_low: u16,
    base_low: u16,

    base_mid: u8,

    pub segment_type: B4,
    pub code_or_data: bool,
    pub dpl: B2,
    pub present: bool,

    limit_hi: B4,
    pub avl: bool,
    pub is_64_bit: bool,
    pub db: bool,
    pub page_granularity: bool,

    base_hi: u8,
}

impl SegmentDescriptor {
    pub fn limit(&self) -> usize {
        ((self.limit_hi() as usize) << 16) | self.limit_low() as usize
    }

    pub fn set_limit(&mut self, limit: usize) {
        assert!(limit < (1 << 20));
        self.set_limit_hi((limit >> 16) as u8);
        self.set_limit_low(limit as u16);
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.set_limit(limit);
        self
    }

    pub fn base(&self) -> usize {
        ((self.base_hi() as usize) << 24)
            | ((self.base_mid() as usize) << 16)
            | (self.base_low() as usize)
    }

    pub fn set_base(&mut self, base: usize) {
        self.set_base_hi((base >> 24) as u8);
        self.set_base_mid((base >> 16) as u8);
        self.set_base_low(base as u16);
    }

    pub fn with_base(mut self, base: usize) -> Self {
        self.set_base(base);
        self
    }
}
