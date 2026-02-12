//! # nano-ros-platform-stm32f4
//!
//! Platform crate for running nano-ros on STM32F4 family microcontrollers
//! with Ethernet.
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
//! - **Hardware modules** — PHY detection, pin configuration, DWT timing
//! - **Node API** — `run_node()`, `Publisher`, `Subscription`
//!
//! Network transport is delegated to `nano-ros-link-smoltcp` which provides
//! the zenoh-pico TCP symbols (`_z_open_tcp`, `_z_read_tcp`, etc.).
//!
//! # Features
//!
//! The crate supports multiple STM32F4 variants via features:
//!
//! - `stm32f407` - STM32F407 (Discovery board)
//! - `stm32f429` - STM32F429 (Nucleo-F429ZI) - default
//! - `stm32f439` - STM32F439
//! - etc.

#![no_std]

// System primitive modules (provide zenoh-pico FFI symbols)
mod clock;
mod libc_stubs;
mod memory;
mod random;
mod sleep;
mod socket;
mod threading;
mod time;

// Hardware modules
pub mod phy;
pub mod pins;
pub mod timing;

// Application modules
mod config;
mod error;
mod node;
mod publisher;
mod subscriber;

// Re-export entry macro
pub use cortex_m_rt::entry;

// Re-export defmt for user logging
pub use defmt;

// Re-export main types
pub use config::Config;
pub use error::{Error, Result};
pub use node::{Node, run_node};
pub use publisher::Publisher;
pub use subscriber::Subscription;
pub use timing::CycleCounter;

// Re-export core traits needed for message type definitions
pub use nano_ros_core::{self, Deserialize, RosMessage, Serialize};

/// Convenient prelude module
///
/// Use with: `use nano_ros_platform_stm32f4::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::error::{Error, Result};
    pub use crate::node::{Node, run_node};
    pub use crate::phy::PhyType;
    pub use crate::pins::PinConfig;
    pub use crate::publisher::Publisher;
    pub use crate::subscriber::Subscription;
    pub use crate::timing::CycleCounter;
    pub use cortex_m_rt::entry;
    pub use defmt::{debug, error, info, trace, warn};
    pub use nano_ros_core::{Deserialize, RosMessage, Serialize};
}
