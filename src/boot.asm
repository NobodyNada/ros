// asmsyntax=gas

// The bootloader! It performs early initialization & loads the kernel from disk into memory.

.section .boot, "ax"
.code16

.global _start
_start:
    cli
    // Set up segment registers
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax

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

    mov dx, 0x1F2
    mov ax, bx
    out dx, al  // read 1 sector at a time

    cld
readloop:
    // Set sector index to ebx
    mov eax, ebx
    mov dx, 0x1F3
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
