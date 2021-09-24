# ros

A JOS reimplementation in Rust, for [CS 444](https://os2.unexploitable.systems/), Fall 2021.

## Setup

ros requires a Rust nightly toolchain with the `rust-src` component installed. If you have [rustup](https://rustup.rs/) installed and configured, it will read the `rust-toolchain.toml` and automatically download and install the correct toolchain.

## Building

`cargo build` will compile and link all source files, but it will not automatically generate a binary image. Use the provided `mkimage.sh` script to build the kernel and generate a bootable image file. Or just use `cargo run`, which will automatically generate a binary image and boot it within `qemu`.
