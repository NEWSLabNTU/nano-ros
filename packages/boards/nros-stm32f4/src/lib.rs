//! # nros-stm32f4
//!
//! Board crate for running nros on STM32F4 family microcontrollers.
//!
//! Handles hardware and transport initialization. Users call `run()` with
//! a closure that receives `&Config` and creates an `Executor` for full
//! API access (publishers, subscriptions, services, actions, timers).
//!
//! # Transport Features
//!
//! - `ethernet` (default) — STM32 MAC + smoltcp TCP/IP stack
//! - `serial` — USART via zpico-serial
//!
//! At least one transport must be enabled.
//!
//! # Architecture
//!
//! This crate depends on `zpico-platform-stm32f4` for system primitives
//! (zenoh-pico FFI symbols, clock, memory, RNG) and either `zpico-smoltcp`
//! (Ethernet) or `zpico-serial` (serial) for the link layer.

#![no_std]
extern crate zpico_platform_shim;

// Application modules
mod config;
#[allow(dead_code)]
mod error;
mod node;
#[cfg(feature = "ethernet")]
pub mod network;

// Re-export entry macro
pub use cortex_m_rt::entry;

// Re-export defmt for user logging
pub use defmt;

// Re-export zpico-platform for direct access to system primitives
pub use nros_platform_stm32f4;

// Re-export main types
pub use config::Config;
pub use node::{init_hardware, run};
pub use nros_platform_stm32f4::timing::CycleCounter;

// Re-export hardware modules from zpico-platform
#[cfg(feature = "ethernet")]
pub use nros_platform_stm32f4::phy;
pub use nros_platform_stm32f4::pins;

/// Convenient prelude module
///
/// Use with: `use nros_stm32f4::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::node::{init_hardware, run};
    pub use cortex_m_rt::entry;
    pub use defmt::{debug, error, info, trace, warn};
    #[cfg(feature = "ethernet")]
    pub use nros_platform_stm32f4::phy::PhyType;
    pub use nros_platform_stm32f4::pins::PinConfig;
    pub use nros_platform_stm32f4::timing::CycleCounter;
}
