//! RMW (ROS Middleware) abstraction layer for nros.
//!
//! This crate provides the middleware-agnostic transport traits that
//! backend crates (`nros-rmw-zenoh`, `nros-rmw-xrce`) implement.
//! Application code depends on these traits, not on a concrete backend,
//! so the transport can be swapped at compile time via Cargo features.
//!
//! # Trait hierarchy
//!
//! ```text
//! Rmw              — top-level factory, creates Sessions
//! └─ Session       — connection lifecycle, creates handles
//!    ├─ Publisher   — publish serialised messages
//!    ├─ Subscriber  — receive messages (polling or callback)
//!    ├─ ServiceServer — request/reply (server side)
//!    └─ ServiceClient — request/reply (client side)
//! ```
//!
//! See [`traits`] for the full trait definitions.
//!
//! # Features
//!
//! - `std` — Enable standard library support
//! - `alloc` — Enable heap allocation
//!
//! # Synchronization Backends
//!
//! - `sync-spin` — Use `spin::Mutex` (default, works everywhere)
//! - `sync-critical-section` — Use critical sections (RTIC/Embassy compatible)

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod sync;
pub mod traits;

#[cfg(feature = "safety-e2e")]
pub mod safety;

// Re-export safety types when feature is enabled
#[cfg(feature = "safety-e2e")]
pub use safety::{IntegrityStatus, SafetyValidator, crc32};

// Re-export main types
pub use traits::{
    ActionInfo, LocatorProtocol, Publisher, QosDurabilityPolicy, QosHistoryPolicy,
    QosReliabilityPolicy, QosSettings, Rmw, RmwConfig, ServiceClientTrait, ServiceInfo,
    ServiceRequest, ServiceServerTrait, Session, SessionMode, Subscriber, TopicInfo, Transport,
    TransportConfig, TransportError, locator_protocol, validate_locator,
};

// Re-export `MessageInfo` from nros-core so backends implementing
// `Subscriber::try_recv_raw_with_info` don't need their own direct
// nros-core dep.
pub use nros_core::MessageInfo;
