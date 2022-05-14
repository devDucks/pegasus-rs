# pegasus-rs
Multiplatform drivers for pegasus equipment written in Rust.

This driver is meant to communicate with all pegasus powerboxes on all major platforms.

# Run locally (UNIX/Windows)
Be sure to have rust installed (if you don't have rust check [here](https://www.rust-lang.org/tools/install) and
simply run on your terminal/cmdshell `cargo run`

# Build a debug version of the program (UNIX/Windows)
in your terminal type `cargo build`

# Build an optimized version of the program AKA the version that will run for real (UNIX/Windows)
in your terminal type `cargo build --release`

# Pegasus PPBA protocol instructions

|Command|Description                                                                       |Response|
|:-:    |:-:                                                                               |:-:     |
|P#     |Status                                                                            |PPBA_OK |
|PE:bbbb|Set Power Status on boot. Every number represents 1-4 power outputs.(0=OFF, 1=ON).|PE:1    |
