//! # nano-ros-platform-qemu
//!
//! Platform crate for running nano-ros on QEMU MPS2-AN385.
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
//! - **Node API** — `run_node()`, `Publisher`, `Subscription`
//!
//! Network transport is delegated to `nano-ros-link-smoltcp` which provides
//! the zenoh-pico TCP symbols (`_z_open_tcp`, `_z_read_tcp`, etc.).

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

// Application modules
mod config;
mod error;
mod node;
mod publisher;
mod subscriber;
pub mod timing;

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
pub use timing::CycleCounter;

// Re-export core traits needed for message type definitions
pub use nano_ros_core::{self, Deserialize, RosMessage, Serialize};

/// Prelude for convenient imports
///
/// Use with: `use nano_ros_platform_qemu::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::error::{Error, Result};
    pub use crate::node::{Node, run_node};
    pub use crate::publisher::Publisher;
    pub use crate::subscriber::Subscription;
    pub use crate::timing::CycleCounter;
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
