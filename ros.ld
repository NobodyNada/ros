SECTIONS {
    .boot 0x7c00 : AT(0x0) {
        *(.boot)
    }

    .kernel 0xf0100000 : AT(0x200) {
        _KERNEL_VIRT_START = .;
        KERNEL_PHYS_START = _KERNEL_VIRT_START - KERNEL_RELOC_BASE;
        *(.text .text.* .rodata .rodata.* .data .data.*)
        _KERNEL_VIRT_END = .;
        KERNEL_PHYS_END = _KERNEL_VIRT_END - KERNEL_RELOC_BASE;
        KERNEL_SIZE_SECTORS = (_KERNEL_VIRT_END-_KERNEL_VIRT_START + 0x1FF) >> 9;

        /*
         * hack to work around https://github.com/rust-lang/compiler-builtins/issues/333
         * when building on macOS we generate Mach-O files but the target OS is not Apple,
         * so the fix doesn't apply
         */
        ___rust_probestack = __rust_probestack;
    }

    .bss : {
        KERNEL_BSS_START = .;
        *(.bss .bss.*)
        KERNEL_BSS_END = .;
    }

    _PHYSALLOC_START = . - KERNEL_RELOC_BASE;

    /DISCARD/ : { *(.eh_*) }
}
