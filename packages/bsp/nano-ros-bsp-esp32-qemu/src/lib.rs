//! # nano-ros-bsp-esp32-qemu
//!
//! Board Support Package for running nano-ros on ESP32-C3 in QEMU
//! (OpenCores Ethernet MAC instead of WiFi).
//!
//! This crate provides a simplified API that abstracts away all networking,
//! and hardware details. Users only need to focus on ROS concepts
//! (publishers, subscriptions, topics).
//!
//! # Example
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//!
//! use nano_ros_bsp_esp32_qemu::prelude::*;
//!
//! mod msg {
//!     use nano_ros_bsp_esp32_qemu::{Deserialize, RosMessage, Serialize, nano_ros_core};
//!     pub struct Int32 { pub data: i32 }
//!     impl Serialize for Int32 {
//!         fn serialize(&self, w: &mut nano_ros_core::CdrWriter)
//!             -> core::result::Result<(), nano_ros_core::SerError> { w.write_i32(self.data) }
//!     }
//!     impl Deserialize for Int32 {
//!         fn deserialize(r: &mut nano_ros_core::CdrReader)
//!             -> core::result::Result<Self, nano_ros_core::DeserError> {
//!             Ok(Self { data: r.read_i32()? })
//!         }
//!     }
//!     impl RosMessage for Int32 {
//!         const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Int32_";
//!         const TYPE_HASH: &'static str = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
//!     }
//! }
//! use msg::Int32;
//!
//! #[entry]
//! fn main() -> ! {
//!     run_node(Config::default(), |node| {
//!         let publisher = node.create_publisher::<Int32>("/chatter")?;
//!
//!         for i in 0i32..10 {
//!             for _ in 0..3 { node.spin_once(10); }
//!             publisher.publish(&Int32 { data: i })?;
//!         }
//!
//!         Ok(())
//!     })
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
/// Use with: `use nano_ros_bsp_esp32_qemu::prelude::*;`
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
