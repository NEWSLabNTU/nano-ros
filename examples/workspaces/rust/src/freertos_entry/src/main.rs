//! Entry pkg for the shared Rust workspace on FreeRTOS QEMU MPS2-AN385.

#![no_std]
#![no_main]

// phase-366 W5.c — this image already declares its own ending and keeps it.
// `nros::panic_to_platform!()` is NOT used here: two providers is a duplicate
// lang item, and an image that has already chosen is exactly what the macro must
// not override.
extern crate panic_semihosting;

nros::main!(panic = "own", launch = "demo_bringup");
