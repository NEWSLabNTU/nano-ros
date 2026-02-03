//! nano-ros BSP for STM32F4 family microcontrollers
//!
//! This crate provides a Board Support Package (BSP) that hides low-level
//! platform details (stm32-eth, smoltcp, GPIO pins) behind a simple API.
//!
//! # Example
//!
//! ```no_run
//! #![no_std]
//! #![no_main]
//!
//! use nano_ros_bsp_stm32f4::prelude::*;
//!
//! #[entry]
//! fn main() -> ! {
//!     run_node(Config::nucleo_f429zi(), |node| {
//!         let publisher = node.create_publisher("/demo")?;
//!
//!         for i in 0u32..10 {
//!             node.spin_once(1000);
//!             publisher.publish(&i.to_le_bytes())?;
//!             defmt::info!("Published: {}", i);
//!         }
//!         Ok(())
//!     })
//! }
//! ```
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

pub mod config;
pub mod node;
pub mod pins;
pub mod platform;

// Re-exports for user convenience
pub use config::Config;
pub use cortex_m_rt::entry;
pub use defmt;
pub use node::{run_node, Node};

/// Result type for BSP operations
pub type Result<T> = core::result::Result<T, Error>;

/// Errors that can occur in BSP operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum Error {
    /// Failed to initialize hardware
    HardwareInit,
    /// Failed to initialize network stack
    NetworkInit,
    /// Failed to connect to zenoh router
    ZenohConnect,
    /// Failed to create publisher
    Publisher,
    /// Failed to create subscriber
    Subscriber,
    /// Failed to publish message
    Publish,
    /// Invalid configuration
    InvalidConfig,
    /// Timeout waiting for operation
    Timeout,
    /// Resource exhausted (buffers full, etc.)
    ResourceExhausted,
}

/// Convenient prelude module
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::node::{run_node, Node};
    pub use crate::pins::PinConfig;
    pub use crate::{Error, Result};
    pub use cortex_m_rt::entry;
    pub use defmt::{debug, error, info, trace, warn};
}

/// Publisher handle for sending messages
#[derive(Debug)]
pub struct Publisher {
    handle: i32,
}

impl Publisher {
    /// Publish data to the topic
    pub fn publish(&self, data: &[u8]) -> Result<()> {
        let ret = unsafe {
            zenoh_pico_shim_sys::zenoh_shim_publish(self.handle, data.as_ptr(), data.len())
        };
        if ret < 0 {
            Err(Error::Publish)
        } else {
            Ok(())
        }
    }
}

/// Subscriber callback type
pub type SubscriberCallback = extern "C" fn(data: *const u8, len: usize, ctx: *mut core::ffi::c_void);

/// Subscriber handle for receiving messages
#[derive(Debug)]
pub struct Subscriber {
    handle: i32,
}
