//! # nros-board-esp32
//!
//! Board crate for running nros on ESP32-C3.
//!
//! Provides a `run()` entry point that initializes hardware and the selected
//! transport (WiFi or serial), then calls user code with the configuration.
//! Users create their own `nros` executor and node inside the callback.
//!
//! # Transport Features
//!
//! - `wifi` (default) — WiFi via esp-radio + smoltcp TCP/IP stack
//! - `serial` — UART via zenoh-pico built-in ESP-IDF serial
//!
//! At least one transport must be enabled.
//!
//! # Architecture
//!
//! This crate depends on `zpico-platform-esp32` for system primitives
//! (zenoh-pico FFI symbols, clock, memory, RNG) and adds hardware
//! initialization on top.

#![no_std]
extern crate zpico_platform_shim;

extern crate alloc;

// Application modules
mod config;
mod node;
#[cfg(feature = "wifi")]
pub mod network;

// Re-export entry macro from esp-hal
pub use esp_hal::main as entry;

// Re-export esp-println for user output
pub use esp_println;

// Re-export esp-bootloader for app descriptor
pub use esp_bootloader_esp_idf;

// Re-export zpico-platform for direct access to system primitives
pub use nros_platform_esp32;

// Re-export main types
pub use config::NodeConfig;
#[cfg(feature = "wifi")]
pub use config::{IpMode, WifiConfig};
pub use node::{init_hardware, run};
pub use nros_platform::BoardConfig;
pub use nros_platform_esp32::timing::MonotonicClock;

// Re-export portable-atomic for safe atomics on riscv32imc (no hardware atomic support).
// ESP32-C3 is single-core, so portable-atomic uses compiler fences.
pub use portable_atomic;

// Re-export critical-section for safe interior mutability in statics
pub use critical_section;

/// Prelude for convenient imports
///
/// Use with: `use nros_board_esp32::prelude::*;`
pub mod prelude {
    pub use crate::config::NodeConfig;
    #[cfg(feature = "wifi")]
    pub use crate::config::{IpMode, WifiConfig};
    pub use crate::node::{init_hardware, run};
    pub use esp_hal::main as entry;
    pub use nros_platform::BoardConfig;
    pub use nros_platform_esp32::timing::MonotonicClock;
}
