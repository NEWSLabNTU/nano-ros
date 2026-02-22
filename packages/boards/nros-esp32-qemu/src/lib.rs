//! # nros-esp32-qemu
//!
//! Board crate for running nros on ESP32-C3 in QEMU
//! (OpenCores Ethernet MAC instead of WiFi).
//!
//! Handles hardware and network initialization. Users call `run()` with
//! a closure that receives `&Config` and creates an `Executor` for full
//! API access (publishers, subscriptions, services, actions, timers).
//!
//! # Architecture
//!
//! This crate depends on `zpico-platform-esp32-qemu` for system primitives
//! (zenoh-pico FFI symbols, clock, memory, RNG) and `zpico-smoltcp` for
//! TCP/IP socket management.

#![no_std]

extern crate alloc;

// Application modules
mod config;
mod node;

// Re-export entry macro from esp-hal
pub use esp_hal::main as entry;

// Re-export esp-println for user output
pub use esp_println;

// Re-export esp-bootloader for app descriptor
pub use esp_bootloader_esp_idf;

// Re-export zpico-platform for direct access to system primitives
pub use zpico_platform_esp32_qemu;

// Re-export main types
pub use config::Config;
pub use node::run;
pub use zpico_platform_esp32_qemu::timing::CycleCounter;

// Re-export portable-atomic for safe atomics on riscv32imc (no hardware atomic support).
// ESP32-C3 is single-core, so portable-atomic uses compiler fences.
pub use portable_atomic;

/// Prelude for convenient imports
///
/// Use with: `use nros_esp32_qemu::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::node::run;
    pub use esp_hal::main as entry;
    pub use zpico_platform_esp32_qemu::timing::CycleCounter;
}
