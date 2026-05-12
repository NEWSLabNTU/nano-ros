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
// Phase 97.1.board-decouple — only force-link when `rmw-zenoh` active.
#[cfg(feature = "rmw-zenoh")]
extern crate zpico_platform_shim;

// Phase 97.3.esp32-qemu — `nros-smoltcp::bridge` references
// `smoltcp_clock_now_ms` as an `extern "C"` symbol. zpico-platform-shim
// supplies it for zenoh-pico builds; DDS-only builds drop that shim
// crate, so provide the same forwarder directly here. Cfg-gated to
// avoid a duplicate-symbol clash when both transports are active.
#[cfg(all(feature = "ethernet", not(any(feature = "rmw-zenoh", feature = "serial"))))]
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    use nros_platform_api::PlatformClock;
    <nros_platform_esp32_qemu::Esp32QemuPlatform as PlatformClock>::clock_ms()
}

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
