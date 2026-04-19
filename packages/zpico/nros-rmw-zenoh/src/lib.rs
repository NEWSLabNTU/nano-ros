//! nros-rmw-zenoh: Zenoh-pico RMW backend for nros
//!
//! This crate provides the zenoh-pico transport implementation,
//! combining the safe Rust API over zenoh-pico FFI with the
//! transport layer that implements nros-rmw traits.
//!
//! # Platform Backends
//!
//! Select one backend via feature flags:
//! - `platform-posix` - Uses POSIX threads, for desktop testing
//! - `platform-zephyr` - Uses Zephyr RTOS threads
//! - `platform-bare-metal` - Uses polling (bare-metal platforms)
//! - `platform-freertos` - Uses FreeRTOS threads + lwIP sockets
//! - `platform-threadx` - Uses ThreadX threads + NetX Duo sockets

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
pub(crate) mod config;
pub mod keyexpr;
pub mod zpico;

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
pub mod shim;

// Re-export zpico types (always available)
pub use zpico::{ZenohId, ZpicoError};

// Re-export platform-gated zpico types
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
pub use zpico::{
    Context, LivelinessToken, Publisher as ZpicoPublisher, Queryable, Subscriber as ZpicoSubscriber,
};

// Re-export shim types when platform feature is enabled
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal",
    feature = "platform-freertos",
    feature = "platform-nuttx",
    feature = "platform-threadx"
))]
pub use shim::{
    MessageInfo, RMW_GID_SIZE, RmwAttachment, Ros2Liveliness, SERVICE_BUFFER_SIZE,
    SUBSCRIBER_BUFFER_SIZE, ZenohPublisher, ZenohRmw, ZenohServiceClient, ZenohServiceServer,
    ZenohSession, ZenohSubscriber, ZenohTransport,
};

// Re-export std-only executor wake functions
#[cfg(all(
    feature = "std",
    any(
        feature = "platform-posix",
        feature = "platform-zephyr",
        feature = "platform-bare-metal"
    )
))]
pub use shim::{signal_executor_wake, wait_for_executor_wake};

// Re-export extension traits
pub use keyexpr::{QosKeyExpr, ServiceKeyExpr, TopicKeyExpr};

// Re-export safety types when feature is enabled
#[cfg(feature = "safety-e2e")]
pub use nros_rmw::{IntegrityStatus, SafetyValidator, crc32};
