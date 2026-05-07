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

pub mod custom_transport;
pub mod event;
pub mod sync;
pub mod traits;

#[cfg(feature = "safety-e2e")]
pub mod safety;

// Phase 108 — status-event surface.
pub use event::{
    CountStatus, DeadlineMissedStatus, EventCallback, EventKind, EventPayload,
    LivelinessChangedStatus, MessageLostStatus, payload_from_raw,
};

// Phase 115.A — runtime-pluggable custom transport vtable.
pub use custom_transport::{
    NROS_TRANSPORT_OPS_ABI_VERSION_V1, NrosTransportOps, peek_custom_transport,
    set_custom_transport, take_custom_transport,
};

// Re-export safety types when feature is enabled
#[cfg(feature = "safety-e2e")]
pub use safety::{IntegrityStatus, SafetyValidator, crc32};

// Re-export main types
pub use traits::{
    ActionInfo, LocatorProtocol, Publisher, QosDurabilityPolicy, QosHistoryPolicy,
    QosLivelinessPolicy, QosPolicyMask, QosReliabilityPolicy, QosSettings, Rmw, RmwConfig,
    ServiceClientTrait, ServiceInfo, ServiceRequest, ServiceServerTrait, Session, SessionMode,
    Subscriber, TopicInfo, Transport, TransportConfig, TransportError, locator_protocol,
    validate_locator,
};

// Re-export `MessageInfo` from nros-core so backends implementing
// `Subscriber::try_recv_raw_with_info` don't need their own direct
// nros-core dep.
pub use nros_core::MessageInfo;

// Phase 99 — zero-copy raw API: SlotLending / SlotBorrowing traits.
// Backends opt in by impl'ing these (and forwarding their own `lending`
// feature to `nros-rmw/lending`).
#[cfg(feature = "lending")]
pub use traits::{SlotBorrowing, SlotLending};
