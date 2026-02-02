//! nano-ros-baremetal: High-level bare-metal API for nano-ros
//!
//! This crate provides a simplified API for running nano-ros on bare-metal
//! embedded systems. It abstracts away the complexity of smoltcp network
//! stack setup, socket management, and zenoh-pico integration.
//!
//! # Example
//!
//! ```ignore
//! use nano_ros_baremetal::{BaremetalNode, NodeConfig};
//! use nano_ros_baremetal::platform::qemu_mps2;
//!
//! // Create platform-specific Ethernet driver
//! let eth = qemu_mps2::create_ethernet([0x02, 0x00, 0x00, 0x00, 0x00, 0x00])?;
//!
//! // Create node with network configuration
//! let mut node = BaremetalNode::new(eth, NodeConfig {
//!     ip: [192, 168, 100, 10],
//!     gateway: [192, 168, 100, 1],
//!     prefix: 24,
//!     zenoh_locator: b"tcp/172.20.0.2:7447\0",
//! })?;
//!
//! // Create publisher
//! let publisher = node.create_publisher(b"demo/topic\0")?;
//!
//! // Publish messages
//! for i in 0..10 {
//!     node.spin_once(10);
//!     publisher.publish(b"Hello!");
//! }
//!
//! node.shutdown();
//! ```
//!
//! # Platform Support
//!
//! Enable platform support via feature flags:
//! - `qemu-mps2`: QEMU MPS2-AN385 with LAN9118 Ethernet

#![no_std]

mod buffers;
mod config;
mod error;
mod node;
mod publisher;
mod subscriber;

pub mod platform;

// Re-export main types
pub use config::NodeConfig;
pub use error::{Error, Result};
pub use node::{create_interface, create_socket_set, BaremetalNode, EthernetDevice};
pub use publisher::Publisher;
pub use subscriber::Subscriber;
