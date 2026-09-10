//! ESP32-C3 QEMU Talker — Entry pkg.
//!
//! `nros::main!()` reads the board (`[image.*] board =
//! "esp32-c3-baremetal"`) from this pkg's `system.toml` (RFC-0098 D3),
//! maps it to `nros_board_esp32_qemu::Esp32QemuEntry`, and emits the
//! `#[esp_hal::main]` boot scaffold that brings up the board, opens the
//! executor, registers this pkg's `Talker` node (its sibling `lib.rs`
//! `nros::node!` export) and spins.
//!
//! Network endpoint / domain come from the same `system.toml`
//! (`[image.*] ip/gateway/locator`, `[system] domain_id`); board MAC
//! defaults live in the board crate.

#![no_std]
#![no_main]

// Panic handler + bootloader app descriptor are crate-root items the
// proc-macro cannot inject; esp-backtrace is link-forced (no leak — it
// declares no application logic).
use esp_backtrace as _;

nros_board_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

nros::main!(panic = "own");
