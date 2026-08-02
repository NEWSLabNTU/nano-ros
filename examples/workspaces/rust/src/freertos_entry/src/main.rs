//! Entry pkg for the shared Rust workspace on FreeRTOS QEMU MPS2-AN385.

#![no_std]
#![no_main]

extern crate panic_semihosting;

nros::main!(model = "demo_bringup:config/system_model.yaml");
