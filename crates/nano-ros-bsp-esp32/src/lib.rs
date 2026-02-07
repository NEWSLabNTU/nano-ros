//! # nano-ros-bsp-esp32
//!
//! Board Support Package for running nano-ros on ESP32-C3 (WiFi).
//!
//! This crate provides a simplified API that abstracts away all WiFi,
//! network stack, and hardware details. Users only need to focus on ROS
//! concepts (publishers, subscribers, topics).
//!
//! # Example
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//!
//! use nano_ros_bsp_esp32::prelude::*;
//!
//! #[entry]
//! fn main() -> ! {
//!     run_node(
//!         NodeConfig::new(WifiConfig::new("MySSID", "MyPassword")),
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
//!
//! # Network Configuration
//!
//! By default, the BSP uses DHCP to acquire an IP address from the WiFi
//! router. Static IP configuration is also supported:
//!
//! ```ignore
//! let config = NodeConfig::new(WifiConfig::new("MySSID", "MyPassword"))
//!     .with_zenoh_locator(b"tcp/10.0.0.1:7447\0")
//!     .with_static_ip([10, 0, 0, 100], 24, [10, 0, 0, 1]);
//!
//! run_node(config, |node| {
//!     // ...
//!     Ok(())
//! });
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
pub use config::{IpMode, NodeConfig, WifiConfig};
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
/// Use with: `use nano_ros_bsp_esp32::prelude::*;`
pub mod prelude {
    pub use crate::config::{IpMode, NodeConfig, WifiConfig};
    pub use crate::error::Error;
    pub use crate::node::{Node, run_node};
    pub use crate::publisher::Publisher;
    pub use crate::subscriber::Subscriber;
    pub use esp_hal::main as entry;
    pub use zenoh_pico_shim_sys::ShimCallback;
}
