//! Entry pkg for the shared Rust workspace on ESP32-C3 QEMU (OpenETH).
//!
//! Phase 225.O — the body is the SAME one-line `nros::main!(model = …)`
//! the native / freertos / threadx / zephyr entries use.
//! `[package.metadata.nros.entry] deploy = "esp32-qemu"` routes the macro
//! onto its `Framework::Esp32` emit branch, which:
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. parses `demo_bringup/launch/system.launch.xml`,
//!   3. emits `talker_pkg::register(runtime)?;` +
//!      `listener_pkg::register(runtime)?;` (launch file = single source
//!      of truth for the node set),
//!   4. emits `#[esp_hal::main] fn main() -> !` that delegates to the
//!      board crate's real-runtime `Esp32QemuEntry::run` — which builds
//!      the `Config`, brings up esp-hal + heap + OpenETH/smoltcp, opens
//!      an `Executor`, wraps it in `ExecutorNodeRuntime`, registers each
//!      launch-named node, and spins.
//!
//! The Entry owns the no_std scaffolding the macro can't supply: the
//! panic handler (`esp-backtrace`) and the ESP-IDF app descriptor
//! (`esp_app_desc!`, via the board crate's re-export). Same shape as the
//! single-node esp32 example's `main.rs`.

#![no_std]
#![no_main]

use esp_backtrace as _;

// ESP-IDF app descriptor — the second-stage bootloader scans for it.
nros_board_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

nros::main!(model = "demo_bringup:config/system_model.yaml");
