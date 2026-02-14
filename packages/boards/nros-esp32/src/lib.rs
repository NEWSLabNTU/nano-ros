//! # nros-esp32
//!
//! Board crate for running nros on ESP32-C3 with WiFi.
//!
//! Provides a simplified node API that abstracts away WiFi, network
//! stack, and hardware details. Users only need to focus on ROS
//! concepts (publishers, subscriptions, topics).
//!
//! # Architecture
//!
//! This crate depends on `zpico-platform-esp32` for system primitives
//! (zenoh-pico FFI symbols, clock, memory, RNG) and adds the nros
//! user-facing API on top.

#![no_std]

extern crate alloc;

// Application modules
mod config;
mod error;
mod node;
mod publisher;
mod subscriber;

// Re-export entry macro from esp-hal
pub use esp_hal::main as entry;

// Re-export esp-println for user output
pub use esp_println;

// Re-export esp-bootloader for app descriptor
pub use esp_bootloader_esp_idf;

// Re-export zpico-platform for direct access to system primitives
pub use zpico_platform_esp32;

// Re-export main types
pub use config::{IpMode, NodeConfig, WifiConfig};
pub use error::Error;
// NOTE: We intentionally do NOT re-export `type Result<T>` publicly.
// The `esp_println::println!` macro uses `?` internally with `core::result::Result<(), fmt::Error>`.
// A `Result<T>` type alias in scope would shadow `core::result::Result`, causing
// "expected 1 generic argument but 2 supplied" errors in any module that uses both.
pub use node::{Node, run_node};
pub use publisher::Publisher;
pub use subscriber::Subscription;
pub use zpico_platform_esp32::timing::CycleCounter;

// Re-export core traits needed for message type definitions
pub use nros_core::{self, Deserialize, RosMessage, Serialize};

// Re-export portable-atomic for safe atomics on riscv32imc (no hardware atomic support).
// ESP32-C3 is single-core, so portable-atomic uses compiler fences.
pub use portable_atomic;

// Re-export critical-section for safe interior mutability in statics
pub use critical_section;

/// Prelude for convenient imports
///
/// Use with: `use nros_esp32::prelude::*;`
pub mod prelude {
    pub use crate::config::{IpMode, NodeConfig, WifiConfig};
    pub use crate::error::Error;
    pub use crate::node::{Node, run_node};
    pub use crate::publisher::Publisher;
    pub use crate::subscriber::Subscription;
    pub use esp_hal::main as entry;
    pub use nros_core::{Deserialize, RosMessage, Serialize};
    pub use zpico_platform_esp32::timing::CycleCounter;
}
