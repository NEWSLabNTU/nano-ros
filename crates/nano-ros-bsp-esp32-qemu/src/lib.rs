//! # nano-ros-bsp-esp32-qemu
//!
//! Board Support Package for running nano-ros on ESP32-C3 in QEMU
//! (OpenCores Ethernet MAC instead of WiFi).
//!
//! This crate provides a simplified API that abstracts away all networking,
//! and hardware details. Users only need to focus on ROS concepts
//! (publishers, subscribers, topics).
//!
//! # Example
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//!
//! use nano_ros_bsp_esp32_qemu::prelude::*;
//!
//! #[entry]
//! fn main() -> ! {
//!     run_node(
//!         Config::default(),
//!         |node| {
//!             let publisher = node.create_publisher(b"demo/esp32\0")?;
//!
//!             for i in 0u32..10 {
//!                 for _ in 0..100 { node.spin_once(10); }
//!                 publisher.publish(&i.to_le_bytes())?;
//!             }
//!
//!             Ok(())
//!         },
//!     )
//! }
//! ```

#![no_std]

extern crate alloc;

// Internal modules (not exposed publicly)
mod bridge;
mod buffers;
mod clock;
mod error;
mod libc_stubs;
mod publisher;
mod subscriber;

// Public modules
mod config;
mod node;

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
pub use subscriber::Subscriber;

// Re-export callback type for subscribers
pub use zenoh_pico_shim_sys::ShimCallback;

/// Prelude for convenient imports
///
/// Use with: `use nano_ros_bsp_esp32_qemu::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::error::Error;
    pub use crate::node::{Node, run_node};
    pub use crate::publisher::Publisher;
    pub use crate::subscriber::Subscriber;
    pub use esp_hal::main as entry;
    pub use zenoh_pico_shim_sys::ShimCallback;
}
