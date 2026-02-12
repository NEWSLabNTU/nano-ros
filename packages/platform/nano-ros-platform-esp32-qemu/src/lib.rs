//! # nano-ros-platform-esp32-qemu
//!
//! Platform crate for running nano-ros on ESP32-C3 in QEMU
//! (OpenCores Ethernet MAC instead of WiFi).
//!
//! Provides all zenoh-pico system symbols in Rust (memory, clock, RNG, sleep,
//! time, threading stubs, socket helpers) and a simplified node API that
//! abstracts away hardware and network stack details.
//!
//! Users only need to focus on ROS concepts (publishers, subscriptions, topics).
//!
//! # Architecture
//!
//! This crate is composed of:
//! - **System primitives** — `z_malloc`, `z_clock_now`, `z_random_u32`, `z_sleep_ms`, etc.
//! - **C library stubs** — `strlen`, `memcpy`, `strtoul`, etc.
//! - **Node API** — `run_node()`, `Publisher`, `Subscription`
//!
//! Network transport is delegated to `nano-ros-link-smoltcp` which provides
//! the zenoh-pico TCP symbols (`_z_open_tcp`, `_z_read_tcp`, etc.).

#![no_std]

extern crate alloc;

// System primitive modules (provide zenoh-pico FFI symbols)
mod clock;
mod libc_stubs;
mod memory;
mod random;
mod sleep;
mod socket;
mod threading;
mod time;

// Application modules
mod config;
mod error;
mod node;
mod publisher;
mod subscriber;
pub mod timing;

// Re-export entry macro from esp-hal
pub use esp_hal::main as entry;

// Re-export esp-println for user output
pub use esp_println;

// Re-export esp-bootloader for app descriptor
pub use esp_bootloader_esp_idf;

// Re-export main types
pub use config::Config;
pub use error::Error;
// NOTE: We intentionally do NOT re-export `type Result<T>` publicly.
// The `esp_println::println!` macro uses `?` internally with `core::result::Result<(), fmt::Error>`.
// A `Result<T>` type alias in scope would shadow `core::result::Result`, causing
// "expected 1 generic argument but 2 supplied" errors in any module that uses both.
pub use node::{Node, run_node};
pub use publisher::Publisher;
pub use subscriber::Subscription;
pub use timing::CycleCounter;

// Re-export core traits needed for message type definitions
pub use nano_ros_core::{self, Deserialize, RosMessage, Serialize};

// Re-export portable-atomic for safe atomics on riscv32imc (no hardware atomic support).
// ESP32-C3 is single-core, so portable-atomic uses compiler fences.
pub use portable_atomic;

/// Prelude for convenient imports
///
/// Use with: `use nano_ros_platform_esp32_qemu::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::error::Error;
    pub use crate::node::{Node, run_node};
    pub use crate::publisher::Publisher;
    pub use crate::subscriber::Subscription;
    pub use crate::timing::CycleCounter;
    pub use esp_hal::main as entry;
    pub use nano_ros_core::{Deserialize, RosMessage, Serialize};
}
