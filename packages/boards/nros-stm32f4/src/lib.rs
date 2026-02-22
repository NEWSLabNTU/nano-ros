//! # nros-stm32f4
//!
//! Board crate for running nros on STM32F4 family microcontrollers
//! with Ethernet.
//!
//! Handles hardware and network initialization. Users call `run()` with
//! a closure that receives `&Config` and creates an `Executor` for full
//! API access (publishers, subscriptions, services, actions, timers).
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

// Re-export entry macro
pub use cortex_m_rt::entry;

// Re-export defmt for user logging
pub use defmt;

// Re-export zpico-platform for direct access to system primitives
pub use zpico_platform_stm32f4;

// Re-export main types
pub use config::Config;
pub use node::run;
pub use zpico_platform_stm32f4::timing::CycleCounter;

// Re-export hardware modules from zpico-platform
pub use zpico_platform_stm32f4::phy;
pub use zpico_platform_stm32f4::pins;

/// Convenient prelude module
///
/// Use with: `use nros_stm32f4::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::node::run;
    pub use cortex_m_rt::entry;
    pub use defmt::{debug, error, info, trace, warn};
    pub use zpico_platform_stm32f4::phy::PhyType;
    pub use zpico_platform_stm32f4::pins::PinConfig;
    pub use zpico_platform_stm32f4::timing::CycleCounter;
}
