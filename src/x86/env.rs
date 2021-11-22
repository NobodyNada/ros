use crate::x86;

pub struct Env {
    pub trap_frame: x86::interrupt::InterruptFrame,
    pub cr3: usize,
}

#[repr(C)]
#[derive(Default, Clone)]
pub struct TaskStateSegment {
    pub link: u16,
    _0: u16,

    pub esp0: u32,
    pub ss0: u16,
    _1: u16,

    pub esp1: u32,
    pub ss1: u16,
    _2: u16,

    pub esp2: u32,
    pub ss2: u16,
    _3: u16,

    pub cr3: u32,
    pub eip: u32,
    pub eflags: u32,
    pub eax: u32,
    pub ecx: u32,
    pub edx: u32,
    pub ebx: u32,
    pub esp: u32,
    pub ebp: u32,
    pub esi: u32,
    pub edi: u32,

    pub es: u16,
    _4: u16,
    pub cs: u16,
    _5: u16,
    pub ss: u16,
    _6: u16,
    pub ds: u16,
    _7: u16,
    pub fs: u16,
    _8: u16,
    pub gs: u16,
    _9: u16,
    pub ldts: u16,
    _10: u16,

    pub iobase: u32,
    pub ssp: u32,
}
