use crate::{
    util::Lazy,
    x86::{interrupt::InterruptFrame, io::pio, mmu},
};
use alloc::vec::Vec;

pub static ELVES: Lazy<Vec<Elf32>> = Lazy::new(find_elves);

fn find_elves() -> Vec<Elf32> {
    // Look past the end of the kernel binary on disk for additional elves
    let mut offset = pio::SECTOR_SIZE + // bootloader
                      unsafe { //kernel
                          core::ptr::addr_of!(mmu::KERNEL_VIRT_END)
                              .offset_from(core::ptr::addr_of!(mmu::KERNEL_VIRT_START)) as usize
                      };
    core::iter::from_fn(|| {
        let elf = read_elf_headers(offset as u32).expect("I/O error")?;
        offset = elf.start_offset as usize + elf.max_offset;
        Some(elf)
    })
    .collect()
}

/// An ELF header.
#[derive(Debug)]
pub struct Elf32 {
    pub start_offset: u32,
    pub program_headers: Vec<ProgramHeader>,
    pub max_offset: usize,
    pub entrypoint: usize,
}

impl Elf32 {
    /// Loads the contents of an ELF file into memory.
    pub fn load(&self) -> Result<InterruptFrame, pio::Error> {
        let mut pio = pio::PIO.take().unwrap();

        // First, map all the memory
        {
            let mut mmu = mmu::MMU.take().unwrap();
            let mmu = &mut *mmu;

            // Unmap userspace first

            for segment in &self.program_headers {
                assert!(
                    segment.vaddr >= (0x1 << 22),
                    "segment.vaddr {:#08x?} is within null region",
                    segment.vaddr
                );
                assert!(
                    segment.vaddr < mmu::KERNEL_RELOC_BASE,
                    "segment.vaddr {:#08x?} is within kernel virtual address space",
                    segment.vaddr
                );
                assert!(
                    segment.vaddr.checked_add(segment.memsize).is_some(),
                    "segment.vaddr {:#08x?} + segment.offset {:#08x?} overflows",
                    segment.vaddr,
                    segment.offset
                );
                assert!(segment.vaddr + segment.memsize <= mmu::KERNEL_RELOC_BASE, "segment.vaddr {:#08x?} + segment.offset {:#08x?} overlaps kernel virtual address space", segment.vaddr, segment.offset);

                let start_page = mmu::page_align_down(segment.vaddr);
                let end_page = mmu::page_align_up(segment.vaddr + segment.memsize).unwrap();
                mmu.mapper.map_zeroed(
                    &mut mmu.allocator,
                    start_page,
                    (end_page - start_page) / mmu::PAGE_SIZE,
                    mmu::mmap::MappingFlags::new()
                        .with_writable(true)
                        .with_user_accessible(true),
                );
            }
        }

        // Then, load the segments into memory
        for segment in &self.program_headers {
            let offset = self.start_offset + segment.offset as u32;
            let mut reader = pio.reader(offset / pio::SECTOR_SIZE as u32);
            reader.prefetch(segment.memsize / pio::SECTOR_SIZE)?;
            let mut reader = reader.skip(offset as usize % pio::SECTOR_SIZE);

            for i in 0..core::cmp::min(segment.filesize, segment.memsize) {
                unsafe {
                    *((segment.vaddr + i) as *mut u8) = reader.next().unwrap()?;
                }
            }
        }

        // Allocate a 32-KiB user stack
        let user_stack_top = {
            let mut mmu = mmu::MMU.take().unwrap();
            let mmu = &mut *mmu;

            let user_stack_bytes = 0x8000;
            let user_stack_pages = user_stack_bytes >> mmu::PAGE_SHIFT;
            let user_stack = mmu
                .mapper
                .find_unused_userspace(user_stack_pages)
                .expect("not enough address space for user stack");
            mmu.mapper.map_zeroed(
                &mut mmu.allocator,
                user_stack,
                user_stack_pages,
                mmu::mmap::MappingFlags::new()
                    .with_writable(true)
                    .with_user_accessible(true),
            );

            user_stack + user_stack_bytes
        };

        // Create an initial trap frame
        Ok(InterruptFrame {
            eip: self.entrypoint,
            cs: mmu::SegmentId::UserCode as usize,
            ds: mmu::SegmentId::UserData as usize,
            es: mmu::SegmentId::UserData as usize,
            fs: mmu::SegmentId::UserData as usize,
            gs: mmu::SegmentId::UserData as usize,
            user_ss: mmu::SegmentId::UserData as usize,
            user_esp: user_stack_top,
            eflags: 0x200, // enable interrupts
            ..Default::default()
        })
    }
}

#[derive(Debug)]
pub struct ProgramHeader {
    offset: usize,
    vaddr: usize,
    filesize: usize,
    memsize: usize,
}

pub fn read_elf_headers(offset: u32) -> Result<Option<Elf32>, pio::Error> {
    let mut pio = pio::PIO.take().unwrap();

    let mut header_reader = pio
        .reader(offset / pio::SECTOR_SIZE as u32)
        .skip(offset as usize % pio::SECTOR_SIZE);

    if header_reader.next().transpose()? != Some(0x7f)      // make sure it's an ELF
        || header_reader.next().transpose()? != Some(0x45)
        || header_reader.next().transpose()? != Some(0x4c)
        || header_reader.next().transpose()? != Some(0x46)
    {
        return Ok(None);
    }

    assert_eq!(header_reader.next().unwrap()?, 0x1, "ELF must be 32-bit");
    assert_eq!(
        header_reader.next().unwrap()?,
        0x1,
        "ELF must be little-endian"
    );
    assert_eq!(header_reader.next().unwrap()?, 0x1, "ELF must be version 1");

    header_reader.nth(8); // skip ABI and padding
    assert_eq!(
        header_reader.next().unwrap()?,
        0x2,
        "ELF must be an executable"
    );
    assert_eq!(
        header_reader.next().unwrap()?,
        0x0,
        "ELF must be an executable"
    );

    fn read_u16<I: Iterator<Item = Result<u8, pio::Error>>>(
        header_reader: &mut I,
    ) -> Result<u16, pio::Error> {
        Ok(
            (header_reader.next().unwrap()? as u16)
                | ((header_reader.next().unwrap()? as u16) << 8),
        )
    }
    fn read_u32<I: Iterator<Item = Result<u8, pio::Error>>>(
        header_reader: &mut I,
    ) -> Result<u32, pio::Error> {
        Ok((read_u16(header_reader)? as u32) | ((read_u16(header_reader)? as u32) << 16))
    }
    assert_eq!(read_u16(&mut header_reader)?, 0x3, "ELF must be for x86");
    assert_eq!(read_u32(&mut header_reader)?, 0x1, "ELF must be v1");

    let entrypoint = read_u32(&mut header_reader)? as usize;
    let ph_offset = read_u32(&mut header_reader)?;
    let sh_offset = read_u32(&mut header_reader)?;

    // skip flags & ehsize
    read_u32(&mut header_reader)?;

    let header_size = read_u16(&mut header_reader)? as usize;

    let ph_entry_size = read_u16(&mut header_reader)?;
    let ph_entry_count = read_u16(&mut header_reader)?;
    let sh_entry_size = read_u16(&mut header_reader)?;
    let sh_entry_count = read_u16(&mut header_reader)?;

    let mut max_offset = core::cmp::max(
        header_size,
        core::cmp::max(
            ph_offset as usize + ph_entry_count as usize * ph_entry_size as usize,
            sh_offset as usize + sh_entry_count as usize * sh_entry_size as usize,
        ),
    );

    // Read program headers
    let mut program_headers = Vec::<ProgramHeader>::new();
    let mut ph_reader = pio
        .reader((offset + ph_offset) / pio::SECTOR_SIZE as u32)
        .skip((offset + ph_offset) as usize % pio::SECTOR_SIZE);

    for _ in 0..ph_entry_count {
        let ph_type = read_u32(&mut ph_reader)?;
        let offset = read_u32(&mut ph_reader)? as usize;
        let vaddr = read_u32(&mut ph_reader)? as usize;
        let _paddr = read_u32(&mut ph_reader)?;
        let filesize = read_u32(&mut ph_reader)? as usize;
        let memsize = read_u32(&mut ph_reader)? as usize;

        max_offset = core::cmp::max(max_offset, offset + filesize);
        // skip the rest of the header
        for _ in 0..(ph_entry_size - 24) {
            ph_reader.next().unwrap()?;
        }

        if ph_type == 0x1 {
            program_headers.push(ProgramHeader {
                offset,
                vaddr,
                filesize,
                memsize,
            });
        }
    }

    // Read just enough of the section headers to determine max_offset
    let mut sh_reader = pio
        .reader((offset + sh_offset) / pio::SECTOR_SIZE as u32)
        .skip((offset + sh_offset) as usize % pio::SECTOR_SIZE);

    for _ in 0..sh_entry_count {
        let _name = read_u32(&mut sh_reader)?;
        let _ty = read_u32(&mut sh_reader)?;
        read_u32(&mut sh_reader)?;
        read_u32(&mut sh_reader)?;
        let offset = read_u32(&mut sh_reader)? as usize;
        let size = read_u32(&mut sh_reader)? as usize;
        max_offset = core::cmp::max(max_offset, offset + size);

        for _ in 0..(sh_entry_size - 24) {
            sh_reader.next().unwrap()?;
        }
    }

    Ok(Some(Elf32 {
        start_offset: offset,
        program_headers,
        max_offset,
        entrypoint,
    }))
}
