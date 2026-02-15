//! nros-rmw-zenoh: Zenoh-pico RMW backend for nros
//!
//! This crate provides the zenoh-pico transport implementation,
//! combining the safe Rust API over zenoh-pico FFI with the
//! shim transport layer that implements nros-rmw traits.
//!
//! # Platform Backends
//!
//! Select one backend via feature flags:
//! - `platform-posix` - Uses POSIX threads, for desktop testing
//! - `platform-zephyr` - Uses Zephyr RTOS threads
//! - `platform-bare-metal` - Uses polling (bare-metal platforms)

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod keyexpr;
pub mod zpico;

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal"
))]
pub mod shim;

// Re-export zpico types (always available)
pub use zpico::{ShimError, ShimZenohId};

// Re-export platform-gated zpico types
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal"
))]
pub use zpico::{
    ShimContext, ShimLivelinessToken, ShimPublisher as ZpicoPublisher, ShimQueryable,
    ShimSubscriber as ZpicoSubscriber,
};

// Re-export shim types when platform feature is enabled
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal"
))]
pub use shim::{
    MessageInfo, RMW_GID_SIZE, RmwAttachment, Ros2Liveliness, SERVICE_BUFFER_SIZE,
    SUBSCRIBER_BUFFER_SIZE, ShimPublisher, ShimServiceClient, ShimServiceServer, ShimSession,
    ShimSubscriber, ShimTransport, ZenohId, ZenohRmw,
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

// Backward compatibility aliases
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal"
))]
pub type ZenohTransport = ShimTransport;
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal"
))]
pub type ZenohSession = ShimSession;
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal"
))]
pub type ZenohPublisher = ShimPublisher;
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal"
))]
pub type ZenohSubscriber = ShimSubscriber;
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal"
))]
pub type ZenohServiceClient = ShimServiceClient;
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal"
))]
pub type ZenohServiceServer = ShimServiceServer;
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-zephyr",
    feature = "platform-bare-metal"
))]
pub type LivelinessToken = ShimLivelinessToken;
