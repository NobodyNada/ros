# ros

A JOS reimplementation in Rust, for [CS 444](https://os2.unexploitable.systems/), Fall 2021.

## Setup

ros requires a Rust nightly toolchain with the `rust-src` component installed. If you have [rustup](https://rustup.rs/) installed and configured, it will read the `rust-toolchain.toml` and automatically download and install the correct toolchain.

## Building

`cargo build` will compile and link all source files, but it will not automatically generate a binary image. Use the provided `mkimage.sh` script to build the kernel and generate a bootable image file. Or just use `cargo run`, which will automatically generate a binary image and boot it within `qemu`. `cargo run` also supports some options:

    cargo run --release         # build with optimizations, reduces binary size by ~10x
    cargo run -- -d             # launch in a debugger
    cargo run -- -n             # launch without a GUI
    cargo run --release -- -n   # recommended for running on os2 servers
    cargo run -- -dn            # recommended for debugging on os2 servers

To run usermode applications, specify the name of the application (or a path to an ELF file), for example:

    cargo run --release -- helloworld

Note that `cargo run` does not currently automatically compile applications, so be sure to `cargo build` first.

Therefore, **the recommended flow for testing lab3 is**:

    cargo build --release                                   # build kernel & all user applications
    cargo run --release -- -n helloworld pagefault count    # run kernel with some test apps
