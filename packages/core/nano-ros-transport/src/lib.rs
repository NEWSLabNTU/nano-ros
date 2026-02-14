//! Transport abstraction layer for nros
//!
//! This crate provides the transport layer abstraction for nros,
//! allowing different backends to be used interchangeably.
//!
//! # Features
//!
//! - `std` - Enable standard library support
//! - `alloc` - Enable heap allocation
//! - `zenoh` - Enable zenoh backend (alias for `platform-posix`)
//! - `shim` - Base shim feature (requires platform selection)
//! - `platform-posix` - POSIX platform (desktop testing)
//! - `platform-zephyr` - Zephyr RTOS platform
//! - `platform-bare-metal` - Bare-metal platform (smoltcp)
//!
//! # Executor Support
//!
//! - `rtic` - RTIC executor support (uses critical sections, no background threads)
//! - `embassy` - Embassy executor support
//! - `polling` - Simple polling mode without background threads
//!
//! # Synchronization Backends
//!
//! - `sync-spin` - Use spin::Mutex (default, works everywhere)
//! - `sync-critical-section` - Use critical sections (RTIC/Embassy compatible)
//!
//! For RTIC applications, enable `rtic` and `sync-critical-section` features.

#![no_std]

// Compile-time check: zenoh requires alloc (zenoh implies platform-posix which works with alloc)
#[cfg(all(feature = "zenoh", not(feature = "alloc")))]
compile_error!(
    "The `zenoh` feature requires `alloc`. Enable the `alloc` feature or use `zenoh` feature which implies `alloc`."
);

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod sync;
pub mod traits;

#[cfg(feature = "shim")]
pub mod shim;

// Re-export main types
pub use traits::{
    ActionInfo, Publisher, QosDurabilityPolicy, QosHistoryPolicy, QosReliabilityPolicy,
    QosSettings, ServiceClientTrait, ServiceInfo, ServiceRequest, ServiceServerTrait, Session,
    SessionMode, Subscriber, TopicInfo, Transport, TransportConfig, TransportError,
};

// Re-export shim types when shim feature is enabled
#[cfg(feature = "shim")]
pub use shim::{
    MessageInfo as ShimMessageInfo, RMW_GID_SIZE as SHIM_RMW_GID_SIZE,
    RmwAttachment as ShimRmwAttachment, Ros2Liveliness as ShimRos2Liveliness, ShimPublisher,
    ShimServiceClient, ShimServiceServer, ShimSession, ShimSubscriber, ShimTransport,
    ZenohId as ShimZenohId,
};

// Re-export nano-ros-transport-zenoh types for liveliness support
#[cfg(feature = "shim")]
pub use nano_ros_transport_zenoh::ShimLivelinessToken;

// Backward compatibility: When "zenoh" feature is enabled, re-export shim types with Zenoh* names
// This allows existing code using ZenohTransport, ZenohSession, etc. to continue working
#[cfg(feature = "zenoh")]
pub use shim::{
    MessageInfo, RMW_GID_SIZE, RmwAttachment, Ros2Liveliness, ShimPublisher as ZenohPublisher,
    ShimServiceClient as ZenohServiceClient, ShimServiceServer as ZenohServiceServer,
    ShimSession as ZenohSession, ShimSubscriber as ZenohSubscriber,
    ShimTransport as ZenohTransport, ZenohId,
};

// Re-export liveliness token with backward-compatible name
#[cfg(feature = "zenoh")]
pub use nano_ros_transport_zenoh::ShimLivelinessToken as LivelinessToken;
