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
extern crate zpico_platform_shim;

extern crate alloc;

// Application modules
mod config;
mod node;
#[cfg(feature = "ethernet")]
pub mod network;

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
pub use nros_platform::BoardConfig;
pub use nros_platform_esp32_qemu::timing::MonotonicClock;

// Re-export portable-atomic for safe atomics on riscv32imc (no hardware atomic support).
// ESP32-C3 is single-core, so portable-atomic uses compiler fences.
pub use portable_atomic;

/// Prelude for convenient imports
///
/// Use with: `use nros_board_esp32_qemu::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::node::{init_hardware, run};
    pub use esp_hal::main as entry;
    pub use nros_platform::BoardConfig;
    pub use nros_platform_esp32_qemu::timing::MonotonicClock;
}
