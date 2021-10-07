ENTRY(_start)

SECTIONS {
    .boot 0x7c00 : AT(0x0) {
        *(.boot)
    }

    .kernel 0xf0100000 : AT(0x200) {
        KERNEL_VIRT_START = .;
        KERNEL_PHYS_START = KERNEL_VIRT_START - KERNEL_RELOC_BASE;
        *(.text .text.* .rodata .rodata.* .data .data.*)
        KERNEL_VIRT_END = .;
        KERNEL_PHYS_END = KERNEL_VIRT_END - KERNEL_RELOC_BASE;
        KERNEL_SIZE_SECTORS = (KERNEL_VIRT_END-KERNEL_VIRT_START + 0x1FF) >> 9;

        /*
         * hack to work around https://github.com/rust-lang/compiler-builtins/issues/333
         * when building on macOS we generate Mach-O files but the target OS is not Apple,
         * so the fix doesn't apply
         */
        ___rust_probestack = __rust_probestack;
    }

    .bss : {
        BSS_START = .;
        *(.bss .bss.*)
        BSS_END = .;
    }

    /DISCARD/ : { *(.eh_*) }
}
