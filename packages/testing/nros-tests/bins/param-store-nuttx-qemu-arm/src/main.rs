//! Issue 1706 — entry of the NuttX parameter-store image. Same shape as
//! `examples/qemu-armv7a-nuttx/rust/talker/src/main.rs`: `nros::main!()` reads
//! the board from `system.toml` and emits the `extern "C" fn main` the RTOS
//! task dispatch calls.
#![no_std]
#![no_main]

nros::main!(panic = "platform");
