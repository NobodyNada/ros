#![no_std]
#![feature(asm)]
#![feature(lang_items)]

pub fn exit() -> ! {
    unsafe {
        asm!("int 0x20");
    }
    unreachable!();
}

pub fn yield_cpu() {
    unsafe {
        asm!("int 0x21");
    }
}

pub fn puts(s: &str) {
    unsafe { asm!("int 0x22", in("eax") s.as_ptr(), in("ebx") s.len()) }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    puts("user panicked\n");
    exit()
}

#[lang = "start"]
fn lang_start<T>(main: fn() -> T, _argc: isize, _argv: *const *const u8) -> isize {
    main();
    exit()
}
