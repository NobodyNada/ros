ENTRY(_start)

SECTIONS {
    .boot 0x7c00 : AT(0x0) {
        *(.boot)
    }

    .kernel 0xf0100000 : AT(0x200) {
        KERNEL_VIRT_START = .;
        KERNEL_PHYS_START = KERNEL_VIRT_START - KERNEL_RELOC_BASE;
        *(.text .text.* .rodata .rodata.* .data .data.* .debug_* .eh_* .cfi_*)
        KERNEL_VIRT_END = .;
        KERNEL_PHYS_END = KERNEL_VIRT_END - KERNEL_RELOC_BASE;
        KERNEL_SIZE_SECTORS = (KERNEL_VIRT_END-KERNEL_VIRT_START + 0x1FF) >> 9;
    }

    .bss : {
        BSS_START = .;
        *(.bss .bss.*)
        BSS_END = .;
    }
}
