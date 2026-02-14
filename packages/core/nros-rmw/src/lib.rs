//! RMW abstraction layer for nros
//!
//! This crate provides the middleware-agnostic transport traits for nros,
//! allowing different backends to be used interchangeably.
//!
//! # Features
//!
//! - `std` - Enable standard library support
//! - `alloc` - Enable heap allocation
//!
//! # Synchronization Backends
//!
//! - `sync-spin` - Use spin::Mutex (default, works everywhere)
//! - `sync-critical-section` - Use critical sections (RTIC/Embassy compatible)
//!
//! For RTIC applications, enable `sync-critical-section` feature.

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
