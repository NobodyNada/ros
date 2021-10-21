#![allow(dead_code, clippy::identity_op)]

use crate::util::Lazy;
use modular_bitfield::prelude::*;

#[bitfield]
#[derive(Copy, Clone)]
#[repr(u64)]
pub struct SegmentDescriptor {
    limit_lo: B16,
    base_lo: B24,

    pub accessed: bool,
    pub rw: bool,
    pub executable: bool,
    pub dc: bool,
    pub system: bool,
    pub privilege: B2,
    pub present: bool,

    limit_hi: B4,

    #[skip]
    __: B2,

    pub big: bool,
    pub page_granularity: bool,

    base_hi: B8,
}

impl SegmentDescriptor {
    fn limit(&self) -> u32 {
        (self.limit_hi() as u32) << 16 | self.limit_lo() as u32
    }

    fn set_limit(&mut self, limit: u32) {
        assert!(limit < (1 << 20));
        self.set_limit_hi((limit >> 16) as u8);
        self.set_limit_lo((limit & 0xFFFF) as u16);
    }

    fn with_limit(mut self, limit: u32) -> Self {
        self.set_limit(limit);
        self
    }

    fn base(&self) -> u32 {
        (self.base_hi() as u32) << 24 | self.base_lo() as u32
    }

    fn set_base(&mut self, base: u32) {
        self.set_base_hi((base >> 24) as u8);
        self.set_base_lo(base & 0xFFFFFF);
    }

    fn with_base(mut self, base: u32) -> Self {
        self.set_base(base);
        self
    }
}

pub static GDT: Lazy<[SegmentDescriptor; 3]> = Lazy::new(|| {
    [
        SegmentDescriptor::new(), // null segment
        SegmentDescriptor::new() // code segment covering the entire address space
            .with_base(0)
            .with_page_granularity(true)
            .with_limit(0x000FFFFF)
            .with_executable(true)
            .with_rw(true)
            .with_privilege(3),
        SegmentDescriptor::new() // data segment covering the entire address space
            .with_base(0)
            .with_page_granularity(true)
            .with_limit(0x000FFFFF)
            .with_executable(false)
            .with_rw(true)
            .with_privilege(3),
    ]
});
