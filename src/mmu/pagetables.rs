#![allow(clippy::identity_op)] // https://github.com/Robbepop/modular-bitfield/issues/62
#![allow(dead_code)] // we won't use all these functions & fields, at least for the first few labs

/// Data structure definitions for dealing with pagetables and page directories.
use core::convert::TryInto;
use modular_bitfield::{error::OutOfBounds, prelude::*};

/// A page directory is an array of page directory entries.
#[repr(C, align(4096))]
pub struct PageDirectory(pub [Pde; 1024]);
/// A pagetable is an array of pagetable entries.
#[repr(C, align(4096))]
pub struct Pagetable(pub [Pte; 1024]);

/// A page directory entry.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct Pde {
    pub raw: u32,
}

impl Pde {
    /// Creates an unmapped PDE.
    pub const fn unmapped() -> Self {
        Self { raw: 0 }
    }
    /// Creates a PDE that maps a 4MB page.
    ///
    /// Requires the Page Size Extension bit of cr4 to be enabled.
    pub const fn mapping(pde: MappingPde) -> Self {
        Self {
            raw: u32::from_le_bytes(pde.into_bytes()) | 0x81,
        }
    }

    /// Creates a PDE that references a pagetable.
    pub const fn pagetable(pde: PagetablePde) -> Self {
        Self {
            raw: u32::from_le_bytes(pde.into_bytes()) | 0x01,
        }
    }

    /// Whether this PDE is mapped.
    pub fn is_present(&self) -> bool {
        self.raw & 1 != 0
    }

    /// Whether this PDE is a direct 4MB mapping.
    pub fn is_mapping(&self) -> bool {
        self.is_present() && self.raw & (1 << 7) != 0
    }

    /// Whether this PDE references a pagetable.
    pub fn is_pagetable(&self) -> bool {
        self.is_present() && self.raw & (1 << 7) == 0
    }

    /// If this PDE is a direct 4MB mapping, returns the mapping info.
    pub fn get_mapping(&self) -> Option<MappingPde> {
        if self.is_mapping() {
            Some(MappingPde::from_bytes(self.raw.to_le_bytes()))
        } else {
            None
        }
    }

    /// If this PDE is a pagetable reference, returns the pagetable info.
    pub fn get_pagetable(&self) -> Option<PagetablePde> {
        if self.is_pagetable() {
            Some(PagetablePde::from_bytes(self.raw.to_le_bytes()))
        } else {
            None
        }
    }
}
impl From<u32> for Pde {
    fn from(raw: u32) -> Self {
        Self { raw }
    }
}
impl From<Pde> for u32 {
    fn from(pde: Pde) -> Self {
        pde.raw
    }
}

/// A page directory entry mapping a 4MB page.
#[bitfield(bits = 32)]
#[derive(Debug, Copy, Clone)]
pub struct MappingPde {
    #[skip]
    __: B1, // present bit

    pub is_writable: bool,
    pub userspace_accessible: bool,
    pub writethrough_enabled: bool,
    pub cache_disabled: bool,
    pub accessed: bool,
    pub is_dirty: bool,
    #[skip]
    __: B1, // identifier bit
    pub is_global: bool,
    #[skip]
    __: B3,

    pub pat_enabled: bool,
    paddr_high: B4,
    #[skip]
    __: B5,
    paddr_low: B10,
}

impl MappingPde {
    pub fn paddr(&self) -> u64 {
        u64::from(self.paddr_high()) << 32 | u64::from(self.paddr_low()) << 22
    }

    pub fn set_paddr_checked(&mut self, paddr: u64) -> Result<(), OutOfBounds> {
        assert!(
            paddr & ((1 << 22) - 1) == 0,
            "paddr must be alignned to a 4MB boundary"
        );
        self.set_paddr_low((paddr as u32 >> 22) as u16);
        self.set_paddr_high_checked((paddr >> 32).try_into().map_err(|_| OutOfBounds)?)
    }

    pub fn set_paddr(&mut self, paddr: u64) {
        self.set_paddr_checked(paddr)
            .expect("physical address must not exceed 40 bits");
    }

    pub fn with_paddr_checked(mut self, paddr: u64) -> Result<Self, OutOfBounds> {
        self.set_paddr_checked(paddr)?;
        Ok(self)
    }

    pub fn with_paddr(mut self, paddr: u64) -> Self {
        self.set_paddr(paddr);
        self
    }
}

/// A page directory entry referencing a pagetable.
#[bitfield(bits = 32)]
#[derive(Debug, Copy, Clone)]
pub struct PagetablePde {
    #[skip]
    __: B1, // present bit

    pub is_writable: bool,
    pub userspace_accessible: bool,
    pub writethrough_enabled: bool,
    pub cache_disabled: bool,
    pub accessed: bool,
    #[skip]
    __: B2, // identifier & unused dirty bit
    #[skip]
    __: B4,

    ptaddr_shifted: B20,
}

impl PagetablePde {
    pub fn ptaddr(&self) -> u32 {
        self.ptaddr_shifted() << 12
    }

    pub fn set_ptaddr(&mut self, ptaddr: u32) {
        assert!(
            ptaddr & ((1 << 12) - 1) == 0,
            "ptaddr must be alignned to a 4KB boundary"
        );
        self.set_ptaddr_shifted(ptaddr >> 12);
    }

    pub fn with_ptaddr(mut self, ptaddr: u32) -> Self {
        self.set_ptaddr(ptaddr);
        self
    }
}

/// A pagetable entry.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct Pte {
    pub raw: u32,
}

impl Pte {
    /// An unmapped PTE.
    pub fn unmapped() -> Self {
        Self { raw: 0 }
    }
    /// A PTE mapping a 4KB page.
    pub fn mapping(pte: MappingPte) -> Self {
        Self {
            raw: u32::from_le_bytes(pte.into_bytes()),
        }
    }

    /// Whether this PTE is mapped.
    pub fn is_present(&self) -> bool {
        self.raw & 1 != 0
    }

    /// Gets the mapping info, if this PTE is mapped.
    pub fn get(&self) -> Option<MappingPte> {
        if self.is_present() {
            Some(MappingPte::from_bytes(self.raw.to_le_bytes()))
        } else {
            None
        }
    }
}
impl From<u32> for Pte {
    fn from(raw: u32) -> Self {
        Self { raw }
    }
}
impl From<Pte> for u32 {
    fn from(pte: Pte) -> Self {
        pte.raw
    }
}

/// A pagetable entry mapping a 4KB page.
#[bitfield(bits = 32)]
#[derive(Debug, Copy, Clone)]
pub struct MappingPte {
    #[skip]
    __: B1, // present bit

    pub is_writable: bool,
    pub userspace_accessible: bool,
    pub writethrough_enabled: bool,
    pub cache_disabled: bool,
    pub accessed: bool,
    pub is_dirty: bool,
    pub pat_enabled: bool,
    pub is_global: bool,

    #[skip]
    __: B3,

    physaddr_shifted: B20,
}

impl MappingPte {
    pub fn physaddr(&self) -> u32 {
        self.physaddr_shifted() << 12
    }

    pub fn set_physaddr(&mut self, physaddr: u32) {
        assert!(
            physaddr & ((1 << 12) - 1) == 0,
            "physaddr must be alignned to a 4KB boundary"
        );
        self.set_physaddr_shifted(physaddr >> 12);
    }

    pub fn with_physaddr(mut self, physaddr: u32) -> Self {
        self.set_physaddr(physaddr);
        self
    }
}
