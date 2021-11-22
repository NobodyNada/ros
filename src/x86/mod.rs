pub mod env;
pub mod interrupt;
pub mod io;
pub mod mmu;

/// Returns the current code segment.
pub fn cs() -> u16 {
    unsafe {
        let result: u16;
        asm!("mov {:x}, cs", out(reg) result, options(nomem, nostack));
        result
    }
}

/// An interrupt or global descriptor table register.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DescriptorTableRegister {
    padding: u16,
    pub size: u16,
    pub paddr: usize,
}

impl DescriptorTableRegister {
    /// Creates a new [IG]DTR.
    pub fn new(size: u16, paddr: usize) -> Self {
        DescriptorTableRegister {
            padding: 0,
            size,
            paddr,
        }
    }

    /// Writes to the interrupt descriptor table register.
    pub fn lidt(&self) {
        unsafe {
            asm!("lidt [{}]", in(reg) &self.size, options(nostack));
        }
    }

    /// Writes to the global descriptor table register.
    pub fn lgdt(&self) {
        unsafe {
            asm!("lgdt [{}]", in(reg) &self.size, options(nostack));
        }
    }

    /// Reads from the interrupt descriptor table register.
    pub fn sidt() -> Self {
        let mut result = Self::new(0, 0);
        unsafe {
            asm!("sidt [{}]", in(reg) &mut result.size, options(nostack));
        }
        result
    }

    /// Reads from the global descriptor table register.
    pub fn sgdt() -> Self {
        let mut result = Self::new(0, 0);
        unsafe {
            asm!("sgdt [{}]", in(reg) &mut result.size, options(nostack));
        }
        result
    }
}
