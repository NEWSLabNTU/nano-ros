//! ESP32-C3 QEMU Listener — Entry pkg.
//!
//! `nros::main!()` reads `[package.metadata.nros.entry] deploy =
//! "qemu-esp32-baremetal"` from this pkg's `Cargo.toml`, maps the
//! deploy key to `nros_board_esp32_qemu::Esp32QemuEntry`, and emits the
//! `#[esp_hal::main]` boot scaffold that brings up the board, opens the
//! executor, registers this pkg's `Listener` node (its sibling `lib.rs`
//! `nros::node!` export) and spins.
//!
//! Network endpoint / domain come from
//! `[package.metadata.nros.deploy.qemu-esp32-baremetal]`; board MAC / IP
//! defaults live in the board crate.

#![no_std]
#![no_main]

// Panic handler + bootloader app descriptor are crate-root items the
// proc-macro cannot inject; esp-backtrace is link-forced (no leak — it
// declares no application logic).
use esp_backtrace as _;

nros_board_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

nros::main!();
