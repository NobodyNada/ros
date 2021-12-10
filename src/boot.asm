// asmsyntax=gas

// The bootloader! It performs early initialization & loads the kernel from disk into memory.

.section .boot, "ax"
.code16

.global _BIOS_MEMORY_MAP
_BIOS_MEMORY_MAP = 0x7e00

.global _start
_start:
    cli
    // Set up segment registers
    xor eax, eax
    mov ds, ax
    mov es, ax
    mov ss, ax

    // Detect available memory
    mov edi, offset _BIOS_MEMORY_MAP
    xor ebx, ebx
    mov edx, 0x534D4150
memloop:
    mov eax, 0xE820
    mov ecx, 0x18
    int 0x15
    // If the BIOS returned only 5 words, initialize the 6th
    mov dword ptr [ecx + edi], 0x1
    jc memdone
    test ebx, ebx
    jz memdone

    // If this entry is valid (size is nonzero), update pointer to point to next entry.
    // Otherwise, just rewrite the same entry again.
    test dword ptr [di + 0x8], 0xffffffff
    jz memloop
    add di, 0x18
    jmp memloop

memdone:
    // Terminate the list with a zero-sized entry
    mov dword ptr [di + 0x28], 0

    // Enable A20
    in al, 0x92
    or al, 2
    out 0x92, al

    // Load GDT & switch to 32-bit protected mode
    lgdt gdtptr
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    // Jump to protected mode code segment
    ljmp 0x8, offset protected
    
protected:
    .code32
    // Set protected mode data segment
    mov eax, 0x10
    mov ds, eax
    mov es, eax
    mov fs, eax
    mov gs, eax
    mov ss, eax

    // Load the kernel from the hard disk
    // See https://wiki.osdev.org/ATA_PIO_Mode for documentation on the hard disk registers
    mov dx, 0x3f6
    mov al, 0x2
    out dx, al  // Disable hard disk interrupts

    mov dx, 0x1f7
waitloop1:  // Wait for the drive to be ready
    in al, dx
    and al, 0xC0 // check busy + data ready
    cmp al, 0x40 // wait until we just see data ready
    jne waitloop1

    // enable logical block addressing mode
    mov dx, 0x1F6
    mov al, 0xE0
    out dx, al

    // Initialize variables
    mov si, offset KERNEL_SIZE_SECTORS  // si = sectors remaining
    mov edi, offset KERNEL_PHYS_START   // edi = destination
    mov ebx, 1                          // ebx = sector index

    cld
readloop:
    // read 1 sector
    mov dx, 0x1F2
    mov al, 1
    out dx, al
    // Set sector index to ebx
    mov eax, ebx
    add dx, 1 // 0x1F3
    out dx, al
    shr eax, 8
    inc dx
    out dx, al
    shr eax, 8
    inc dx
    out dx, al

    add dx, 2 // 0x1F7

    mov ax, 0x20    // Issue the read command
    out dx, al

    // Poll for ready signal, just like before
waitloop2:
    in al, dx
    and al, 0xC0
    cmp al, 0x40
    jne waitloop2

    sub dx, 7
    mov cx, 0x100

    // copy cx words from the drive to edi
    rep insw 

    // next sector!
    inc ebx
    dec si
    jnz readloop    // are we done?
    
    // yes, now boot the kernel!
    jmp _kentry

.align 4
gdt:
    // Null descriptor
    .byte 0,0,0,0
    .byte 0,0,0,0
    // Code segment: map all address space as readable & executable
    .word 0xFFFF, 0x0000
    .byte 0x00, 0x9A, 0xCF, 0x00
    // Code segment: map all address space as readable & writable
    .word 0xFFFF, 0x0000
    .byte 0x00, 0x92, 0xCF, 0x00
gdtptr:
    .word 8*3 - 1   // size
    .long gdt       // ptr


panic:
    jmp panic

.org 510
.word 0xAA55
