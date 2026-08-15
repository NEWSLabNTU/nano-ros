//! Phase 213.C.2 — N.9 macro shape.
//!
//! Entry pkg for the NuttX QEMU ARM service-server. See `talker_entry/src/main.rs`
//! for the full macro lifecycle docs.

// phase-359 W7 — NuttX is a `no_std` family now, so this entry takes the same
// shape as the other C-runtime families' leaves (FreeRTOS, threadx-linux):
// `no_main`, with `nros::main!()` emitting the `extern "C" fn main` that the
// RTOS task dispatch calls. Previously libstd's `lang_start` supplied that
// symbol, which is the one thing compiling the standard library bought here.
#![no_std]
#![no_main]

nros::main!();
