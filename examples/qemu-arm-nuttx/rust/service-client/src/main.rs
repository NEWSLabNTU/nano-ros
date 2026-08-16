//! Phase 213.C.2 — N.9 macro shape.
//!
//! Entry pkg for the NuttX QEMU ARM service-client. See `talker_entry/src/main.rs`
//! for the full macro lifecycle docs.

// phase-359 W7 — NuttX is a `no_std` family now, so this entry takes the same
// shape as the other C-runtime families' leaves (FreeRTOS, threadx-linux):
// `no_main`, with `nros::main!()` emitting the `extern "C" fn main` that the
// RTOS task dispatch calls. Previously libstd's `lang_start` supplied that
// symbol, which is the one thing compiling the standard library bought here.
#![no_std]
#![no_main]

// phase-366 W5.c — this image's ending, declared by the image. Forwards to the
// board's `nros_platform_panic` (`nros: PANIC <msg>`, then exit(1), which is the
// status the e2e harness expects). Swap for `use panic_halt as _;` or a handler
// that logs and reboots.
//
// In `main.rs` here, not `lib.rs`: these packages are `crate-type = ["rlib"]`
// only — phase-359 W7 dropped `staticlib` precisely because a staticlib is a
// FINAL artifact needing its own lang item. The bin is the final artifact.
nros::panic_to_platform!();

nros::main!();
