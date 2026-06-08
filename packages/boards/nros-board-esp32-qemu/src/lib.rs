//! # nros-board-esp32-qemu
//!
//! Board crate for running nros on ESP32-C3 in QEMU
//! (OpenCores Ethernet MAC instead of WiFi).
//!
//! # Transport Features
//!
//! - `ethernet` (default) — OpenETH + smoltcp TCP/IP stack
//! - `serial` — zenoh-pico built-in serial (no additional deps)
//!
//! At least one transport must be enabled.
//!
//! # Architecture
//!
//! This crate depends on `zpico-platform-esp32-qemu` for system primitives
//! (zenoh-pico FFI symbols, clock, memory, RNG) and either `nros-smoltcp`
//! (Ethernet) or zenoh-pico's built-in serial for the link layer.

#![no_std]
// `smoltcp_clock_now_ms` (referenced by `nros-smoltcp::bridge`) is
// provided by `zpico-sys`'s `platform_aliases.c`, which forwards to
// `nros_platform_time_now_ms`. Phase 129 retired the per-board override.

extern crate alloc;

// Application modules
mod config;
#[cfg(feature = "ethernet")]
pub mod network;
mod node;

// Phase 225.O — real-runtime `BoardEntry` shim for the workspace Entry
// macro (`nros::main!(launch = …)`). Needs an RMW backend to register +
// open a session, so it is gated on the default `rmw-zenoh` feature
// (which pulls the `nros` + `nros-rmw-zenoh` deps it uses).
#[cfg(feature = "rmw-zenoh")]
mod board_entry;

// Re-export entry macro from esp-hal
pub use esp_hal::main as entry;

// Re-export esp-println for user output
pub use esp_println;

// Re-export esp-bootloader for app descriptor
pub use esp_bootloader_esp_idf;

// Re-export zpico-platform for direct access to system primitives
pub use nros_platform_esp32_qemu;

// Re-export main types
pub use config::Config;
pub use node::{init_hardware, run};
// Phase 225.O — workspace Entry board ZST (real-runtime `BoardEntry`).
#[cfg(feature = "rmw-zenoh")]
pub use board_entry::Esp32QemuEntry;
pub use nros_platform::BoardConfig;
pub use nros_platform_esp32_qemu::timing::MonotonicClock;

// Re-export portable-atomic for safe atomics on riscv32imc (no hardware atomic support).
// ESP32-C3 is single-core, so portable-atomic uses compiler fences.
pub use portable_atomic;

// Re-export nros-smoltcp so Ethernet examples can read the Phase 127.A
// poll-diagnostic counters without adding a second direct dependency.
#[cfg(feature = "ethernet")]
pub use nros_smoltcp;

/// Prelude for convenient imports
///
/// Use with: `use nros_board_esp32_qemu::prelude::*;`
pub mod prelude {
    pub use crate::{
        config::Config,
        node::{init_hardware, run},
    };
    pub use esp_hal::main as entry;
    pub use nros_platform::BoardConfig;
    pub use nros_platform_esp32_qemu::timing::MonotonicClock;
}
