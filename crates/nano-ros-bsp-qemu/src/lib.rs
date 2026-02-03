//! # nano-ros-bsp-qemu
//!
//! Board Support Package for running nano-ros on QEMU MPS2-AN385.
//!
//! This crate provides a simplified API that abstracts away all hardware
//! and network stack details. Users only need to focus on ROS concepts
//! (publishers, subscribers, topics).
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
//! #[entry]
//! fn main() -> ! {
//!     run_node(Config::default(), |node| {
//!         let publisher = node.create_publisher(b"demo/topic\0")?;
//!
//!         for _ in 0..10 {
//!             node.spin_once(10);
//!             publisher.publish(b"Hello from QEMU!")?;
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
pub use subscriber::Subscriber;

// Re-export callback type for subscribers
pub use zenoh_pico_shim_sys::ShimCallback;

/// Prelude for convenient imports
///
/// Use with: `use nano_ros_bsp_qemu::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::error::{Error, Result};
    pub use crate::node::{Node, run_node};
    pub use crate::publisher::Publisher;
    pub use crate::subscriber::Subscriber;
    pub use cortex_m_rt::entry;
    pub use zenoh_pico_shim_sys::ShimCallback;
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
