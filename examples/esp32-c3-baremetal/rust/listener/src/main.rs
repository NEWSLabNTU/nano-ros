//! ESP32-C3 QEMU Listener — Entry pkg.
//!
//! `nros::main!()` reads the board (`[image.*] board =
//! "esp32-c3-baremetal"`) from this pkg's `system.toml` (RFC-0098 D3),
//! maps it to `nros_board_esp32_qemu::Esp32QemuEntry`, and emits the
//! `#[esp_hal::main]` boot scaffold that brings up the board, opens the
//! executor, registers this pkg's `Listener` node (its sibling `lib.rs`
//! `nros::node!` export) and spins.
//!
//! Network endpoint / domain come from the same `system.toml`
//! (`[image.*] ip/gateway/locator`, `[system] domain_id`); board MAC
//! defaults live in the board crate.

#![no_std]
#![no_main]

// The panic handler (`esp-backtrace`) and the bootloader app descriptor
// (`esp_app_desc!`) are the board's crate-root items; the generated selection
// package carries them (issue 1305), so this file names no board crate.

nros::main!(panic = "own");
