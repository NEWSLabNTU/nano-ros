//! Entry pkg for the shared Rust workspace on FreeRTOS QEMU MPS2-AN385.

#![no_std]
#![no_main]

extern crate panic_semihosting;

nros::main!(launch = "demo_bringup:system.launch.xml");
