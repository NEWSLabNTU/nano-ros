//! # nros-board-esp32s3
//!
//! Board crate for running nros on the ESP32-S3 (Xtensa LX7).
//!
//! Serial transport only (zenoh-pico built-in serial): the S3 has no
//! QEMU NIC like the C3-under-QEMU board, and WiFi (esp-wifi) is a
//! follow-up. Depends on `nros-platform-esp32s3` for system primitives
//! (clock, memory, RNG, the platform C ABI).

#![no_std]

extern crate alloc;

mod config;
mod node;

// Re-export the esp-hal entry macro + support crates the generated /
// hand-written app needs at the crate root.
pub use esp_hal::main as entry;
pub use esp_println;
pub use esp_bootloader_esp_idf;
pub use nros_platform_esp32s3;
pub use portable_atomic;

pub use config::Config;
pub use node::{init_hardware, run};
pub use nros_platform::BoardConfig;

/// Prelude: `use nros_board_esp32s3::prelude::*;`
pub mod prelude {
    pub use crate::{
        config::Config,
        node::{init_hardware, run},
    };
    pub use esp_hal::main as entry;
    pub use nros_platform::BoardConfig;
}
