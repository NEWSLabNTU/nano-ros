//! # nros-qemu
//!
//! Board crate for running nros on QEMU MPS2-AN385.
//!
//! Provides a simplified node API that abstracts away hardware and
//! network stack details. Users only need to focus on ROS concepts
//! (publishers, subscriptions, topics).
//!
//! # Architecture
//!
//! This crate depends on `zpico-platform-qemu` for system primitives
//! (zenoh-pico FFI symbols, clock, memory, RNG) and uses `nros-rmw-zenoh`
//! for the transport layer.

#![no_std]

// Application modules
mod config;
mod error;
mod node;
mod publisher;
mod subscriber;

// Re-export entry macro
pub use cortex_m_rt::entry;

// Re-export semihosting for println! macro
pub use cortex_m_semihosting;

// Re-export zpico-platform for direct access to system primitives
pub use zpico_platform_qemu;

// Re-export main types
pub use config::Config;
pub use error::{Error, Result};
pub use node::{Node, run_node};
pub use publisher::Publisher;
pub use subscriber::Subscription;
pub use zpico_platform_qemu::timing::CycleCounter;

// Re-export core traits needed for message type definitions
pub use nros_core::{self, Deserialize, RosMessage, Serialize};

/// Prelude for convenient imports
///
/// Use with: `use nros_qemu::prelude::*;`
pub mod prelude {
    pub use crate::config::Config;
    pub use crate::error::{Error, Result};
    pub use crate::node::{Node, run_node};
    pub use crate::publisher::Publisher;
    pub use crate::subscriber::Subscription;
    pub use cortex_m_rt::entry;
    pub use nros_core::{Deserialize, RosMessage, Serialize};
    pub use zpico_platform_qemu::timing::CycleCounter;
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
