//! # nano-ros-bsp-qemu
//!
//! Board Support Package for running nano-ros on QEMU MPS2-AN385.
//!
//! This crate provides a simplified API that abstracts away all hardware
//! and network stack details. Users only need to focus on ROS concepts
//! (publishers, subscriptions, topics).
//!
//! # Example
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//!
//! use nano_ros_bsp_qemu::prelude::*;
//! use panic_semihosting as _; // Required panic handler
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
//!     run_node(Config::default(), |node| {
//!         let publisher = node.create_publisher::<Int32>("/chatter")?;
//!
//!         for i in 0i32..10 {
//!             node.spin_once(10);
//!             publisher.publish(&Int32 { data: i })?;
//!         }
//!
//!         Ok(())
//!     })
//! }
//! ```
//!
//! # Network Configuration
//!
//! By default, the BSP assumes direct TAP networking to the host:
//! - IP: 192.0.3.10/24
//! - Gateway: 192.0.3.1
//! - Zenoh router: tcp/192.0.3.1:7447
//!
//! For Docker mode (enable `docker` feature):
//! - IP: 192.168.100.10/24
//! - Gateway: 192.168.100.1
//! - Zenoh router: tcp/172.20.0.2:7447
//!
//! # Custom Configuration
//!
//! ```ignore
//! let config = Config::default()
//!     .with_ip([10, 0, 0, 100])
//!     .with_gateway([10, 0, 0, 1])
//!     .with_zenoh_locator(b"tcp/10.0.0.1:7447\0");
//!
//! run_node(config, |node| {
//!     // ...
//!     Ok(())
//! });
//! ```

#![no_std]

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

// Re-export entry macro
pub use cortex_m_rt::entry;

// Re-export semihosting for println! macro
pub use cortex_m_semihosting;

// Re-export main types
pub use config::Config;
pub use error::{Error, Result};
pub use node::{Node, run_node};
pub use publisher::Publisher;
pub use subscriber::Subscription;

// Re-export core traits needed for message type definitions
pub use nano_ros_core::{self, Deserialize, RosMessage, Serialize};

/// Prelude for convenient imports
///
/// Use with: `use nano_ros_bsp_qemu::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::error::{Error, Result};
    pub use crate::node::{Node, run_node};
    pub use crate::publisher::Publisher;
    pub use crate::subscriber::Subscription;
    pub use cortex_m_rt::entry;
    pub use nano_ros_core::{Deserialize, RosMessage, Serialize};
}

/// Print to QEMU semihosting console
#[macro_export]
macro_rules! println {
    () => {
        $crate::cortex_m_semihosting::hprintln!()
    };
    ($($arg:tt)*) => {
        $crate::cortex_m_semihosting::hprintln!($($arg)*)
    };
}

/// Exit QEMU with success status
pub fn exit_success() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);
    #[allow(clippy::empty_loop)]
    loop {
        cortex_m::asm::wfi();
    }
}

/// Exit QEMU with failure status
pub fn exit_failure() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_FAILURE);
    #[allow(clippy::empty_loop)]
    loop {
        cortex_m::asm::wfi();
    }
}
