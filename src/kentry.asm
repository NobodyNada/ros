// asmsyntax=gas

// The entry point of the kernel. Sets up the boot pagetable &
// initializes stack before jumping into Rust.

.section .text.kentry, "ax"

// The virtual memory address offset of kernelspace. For example:
// virtual address 0xf1234567 corresponds to physical address 0x01234567.
// NOTE: duplicated in mmu/mod.rs
.global KERNEL_RELOC_BASE
KERNEL_RELOC_BASE = 0xf0000000;

// The assembler/linker thinks we're running with virtual memory,
// so we have to be kinda careful here.

.global _kentry
_kentry = kentry - KERNEL_RELOC_BASE;
kentry:
    // Load the page directory & enable virtual memory.
    mov eax, offset _BOOT_PAGE_DIRECTORY - KERNEL_RELOC_BASE;
    mov cr3, eax

    mov eax, 0x10   // Enable page size extensions
    mov cr4, eax

    mov eax, cr0
    or eax, 0x80010001 // paging + write protect + protected mode
    mov cr0, eax

    // We are now running with virtual memory enabled, but the PC points within
    // the identity mapping at the bottom of RAM rather than the kernel's
    // virtual address space at the top of RAM.
    // Jump to the correct virtual addresss.

    mov eax, offset relocated
    jmp eax
relocated:

    // Zero the bss segment
    mov edi, offset KERNEL_BSS_START
bss:
    mov dword ptr [edi], 0
    add edi, 4
    cmp edi, offset KERNEL_BSS_END
    jl bss

    // Set up the stack
    mov esp, offset _KERNEL_STACK_TOP
    mov ebp, 0

    // and we're ready to run Rust!
    call _main  // (should never return, but we want to 'call' it anyway for the sake of stack unwinding)
    jmp _halt

// Allocate some space for the kernel stack within the bss segment.
.section .bss.kernel_stack, "w"
.align 4096
.global _KERNEL_STACK_SIZE, _KERNEL_STACK_BOTTOM, _KERNEL_STACK_TOP
_KERNEL_STACK_SIZE = 0x8000 // 32 KiB/8 pages
_KERNEL_STACK_BOTTOM: .space _KERNEL_STACK_SIZE
_KERNEL_STACK_TOP:
