# ros

A JOS-like operating system kernel written in Rust, for [CS 444](https://os2.unexploitable.systems/), Fall 2021.

## Setup

ros requires a Rust nightly toolchain with the `rust-src` component installed. If you have [rustup](https://rustup.rs/) installed and configured, it will read the `rust-toolchain.toml` and automatically download and install the correct toolchain.

## Building and running

First, compile and link all source files with `cargo build`:

    cargo build             # for a debug build
    cargo build --release   # for a release build


Then, use `cargo run` to execute the kernel with a list of user programs:

    cargo run -- smallersh catline wc [...]             # to run a debug build
    cargo run --release -- smallersh catline wc [...]   # to run a release build
    cargo run --release -- -n [programs...]             # to run in release mode with no GUI
    cargo run -- -d [programs...]                       # to run in a debugger

The provided `smallersh` executable implements a minimal "shell" that allows you to interactively launch programs. Programs are referenced by index: if you ran ROS with the programs `smallersh catline wc`, `smallersh` would be program 0, `catline` woudl be program 1, and `wc` would be program 2. `smallersh` supports pipes (with the `|` operator) and background execution (with the `&` operator). For example:

    cargo build --release
    cargo run --release -- smallersh catline wc helloworld spin
    [...]
    > 4 & 3         # spin & helloworld
    Hello, world!
    Process 3 exited.
    > 1 | 2         # catline | wc
    The quick brown fox jumps over the lazy dog.
    Process 4 exited.
    1 9 45
    Process 5 exited.
    >

The following user programs are included (under the `src/bin`) directory:

Program|Description
-------|-----------
cat|Copies standard input to standard output until end-of-file is reached.
catline|Copies one line from stdin to stdout.
count|Counts from 0 to 9.
forktest|A simple test to ensure the `fork` syscall works.
helloworld|Hello, world
pagefault|Dereferences a null pointer to test the pagefault handler.
pipetest|Reads and writes to a pipe.
smallersh|Small small shell
spin|Spins forever, to test preemption.
wc|Counts characters, words, and lines.
yield|Calls the `yield` syscall in a loop.
