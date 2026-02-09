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
//! struct Int32 { data: i32 }
//! impl Serialize for Int32 {
//!     fn serialize(&self, w: &mut nano_ros_core::CdrWriter) -> Result<(), nano_ros_core::SerError> {
//!         w.write_i32(self.data)
//!     }
//! }
//! impl Deserialize for Int32 {
//!     fn deserialize(r: &mut nano_ros_core::CdrReader) -> Result<Self, nano_ros_core::DeserError> {
//!         Ok(Self { data: r.read_i32()? })
//!     }
//! }
//! impl RosMessage for Int32 {
//!     const TYPE_NAME: &'static str = "std_msgs::msg::dds_::Int32_";
//!     const TYPE_HASH: &'static str = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000";
//! }
//!
//! #[entry]
//! fn main() -> ! {
//!     run_node(Config::nucleo_f429zi(), |node| {
//!         let publisher = node.create_publisher::<Int32>("/chatter")?;
//!
//!         for i in 0i32..10 {
//!             node.spin_once(1000);
//!             publisher.publish(&Int32 { data: i })?;
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
pub mod phy;
pub mod pins;
pub mod platform;

// Re-exports for user convenience
pub use config::Config;
pub use cortex_m_rt::entry;
pub use defmt;
pub use node::{Node, run_node};

// Re-export core traits needed for message type definitions
pub use nano_ros_core::{self, Deserialize, RosMessage, Serialize};

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
    /// Topic keyexpr too long for internal buffer
    TopicTooLong,
    /// CDR serialization buffer too small
    BufferTooSmall,
    /// CDR serialization failed
    Serialize,
}

/// Convenient prelude module
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::node::{Node, run_node};
    pub use crate::phy::PhyType;
    pub use crate::pins::PinConfig;
    pub use crate::{Error, Result};
    pub use cortex_m_rt::entry;
    pub use defmt::{debug, error, info, trace, warn};
    pub use nano_ros_core::{Deserialize, RosMessage, Serialize};
}

/// Publisher handle for sending typed messages
pub struct Publisher<M: RosMessage> {
    handle: i32,
    _marker: core::marker::PhantomData<M>,
}

impl<M: RosMessage> Publisher<M> {
    /// Publish a typed message (CDR-serialized automatically)
    ///
    /// Uses a 256-byte stack buffer. For larger messages, use
    /// [`publish_with_buffer`](Self::publish_with_buffer).
    pub fn publish(&self, msg: &M) -> Result<()> {
        self.publish_with_buffer::<256>(msg)
    }

    /// Publish a typed message with a custom stack buffer size
    pub fn publish_with_buffer<const BUF: usize>(&self, msg: &M) -> Result<()> {
        let mut buf = [0u8; BUF];
        let mut writer =
            nano_ros_core::CdrWriter::new_with_header(&mut buf).map_err(|_| Error::BufferTooSmall)?;
        msg.serialize(&mut writer)
            .map_err(|_| Error::Serialize)?;
        self.publish_raw(writer.as_slice())
    }

    /// Publish pre-encoded CDR bytes (internal)
    fn publish_raw(&self, data: &[u8]) -> Result<()> {
        let ret = unsafe {
            zenoh_pico_shim_sys::zenoh_shim_publish(self.handle, data.as_ptr(), data.len())
        };
        if ret < 0 { Err(Error::Publish) } else { Ok(()) }
    }
}

/// Subscription handle for receiving typed messages
pub struct Subscription<M: RosMessage> {
    #[allow(dead_code)] // Handle kept for future use (e.g., undeclare)
    handle: i32,
    _marker: core::marker::PhantomData<M>,
}

/// Generic trampoline: deserializes CDR and calls user's typed `fn(&M)`
pub(crate) extern "C" fn subscription_trampoline<M: RosMessage>(
    data: *const u8,
    len: usize,
    ctx: *mut core::ffi::c_void,
) {
    let callback: fn(&M) = unsafe { core::mem::transmute(ctx) };
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    if let Ok(mut reader) = nano_ros_core::CdrReader::new_with_header(bytes) {
        if let Ok(msg) = M::deserialize(&mut reader) {
            callback(&msg);
        }
    }
}
