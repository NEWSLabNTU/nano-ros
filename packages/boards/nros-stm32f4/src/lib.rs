//! # nros-stm32f4
//!
//! Board crate for running nros on STM32F4 family microcontrollers
//! with Ethernet.
//!
//! Provides a simplified node API that abstracts away hardware and
//! network stack details. Users only need to focus on ROS concepts
//! (publishers, subscriptions, topics).
//!
//! # Architecture
//!
//! This crate depends on `zpico-platform-stm32f4` for system primitives
//! (zenoh-pico FFI symbols, clock, memory, RNG) and adds the nros
//! user-facing API on top.

#![no_std]

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

// Re-export zpico-platform for direct access to system primitives
pub use zpico_platform_stm32f4;

// Re-export main types
pub use config::Config;
pub use error::{Error, Result};
pub use node::{Node, run_node};
pub use publisher::Publisher;
pub use subscriber::Subscription;
pub use zpico_platform_stm32f4::timing::CycleCounter;

// Re-export hardware modules from zpico-platform
pub use zpico_platform_stm32f4::phy;
pub use zpico_platform_stm32f4::pins;

// Re-export core traits needed for message type definitions
pub use nros_core::{self, Deserialize, RosMessage, Serialize};

/// Convenient prelude module
///
/// Use with: `use nros_stm32f4::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::error::{Error, Result};
    pub use crate::node::{Node, run_node};
    pub use crate::publisher::Publisher;
    pub use crate::subscriber::Subscription;
    pub use cortex_m_rt::entry;
    pub use defmt::{debug, error, info, trace, warn};
    pub use nros_core::{Deserialize, RosMessage, Serialize};
    pub use zpico_platform_stm32f4::phy::PhyType;
    pub use zpico_platform_stm32f4::pins::PinConfig;
    pub use zpico_platform_stm32f4::timing::CycleCounter;
}
