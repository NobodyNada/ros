fn main() {
    // If building the kernel, use the kernel linker script.
    println!("cargo:rustc-link-arg-bin=ros=-Tros.ld");

    // Use _main as the entry point
    println!("cargo:rustc-link-arg=-e");
    println!("cargo:rustc-link-arg=_main");
    // ...unless we're building the kernel, in which case override that to _start
    println!("cargo:rustc-link-arg-bin=ros=-e");
    println!("cargo:rustc-link-arg-bin=ros=_start");
}
