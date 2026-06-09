//! Core types, traits, and abstractions for nros
//!
//! This crate provides the foundational types and traits for nros:
//! - `RosMessage` trait for message types
//! - `RosService` trait for service types
//! - `RosAction` trait for action types
//! - `ServiceServer` and `ServiceClient` for service communication
//! - Time and Duration types
//! - Error types

#![no_std]

#[cfg(feature = "std")]
extern crate std;

// `std` implies the `alloc` crate; pull it in for either so the `heap`
// re-export (used by generated `mode = "heap"` message fields, RFC-0033) is
// available whenever an allocator is.
#[cfg(any(feature = "alloc", feature = "std"))]
extern crate alloc;

pub mod action;
pub mod clock;
pub mod error;
pub mod lifecycle;
pub mod logger;
pub mod message_info;
pub mod service;
pub mod time;
pub mod types;

pub use action::{
    ActionClient, ActionServer, CancelResponse, GoalId, GoalInfo, GoalResponse, GoalStatus,
    GoalStatusStamped, RosAction,
};
pub use clock::{Clock, ClockType};
pub use error::{
    ErrorContext, NanoRosError, NanoRosErrorFilter, NestedError, RclReturnCode, TakeFailedAsNone,
};
pub use lifecycle::{LifecycleState, LifecycleTransition, TransitionResult};
pub use logger::{Logger, OnceFlag};
pub use message_info::{MessageInfo, PUBLISHER_GID_SIZE, RawMessageInfo};
pub use service::{ServiceCallback, ServiceClient, ServiceRequest, ServiceResult, ServiceServer};
pub use time::{Duration, Time};
pub use types::{BorrowedMessage, RosMessage, RosService};

// Re-export serdes types for convenience
pub use nros_serdes::{
    CdrReader, CdrWriter, DeserError, Deserialize, DeserializeBorrowed, SerError, Serialize,
};

// Re-export heapless for generated message types
pub use heapless;

/// Heap-backed containers for generated `mode = "heap"` message fields
/// (RFC-0033). Available whenever an allocator is (the `alloc` or `std`
/// feature). Generated code refers to `nros_core::heap::{Vec, String}` so the
/// same path works in both crate and inline (`build.rs`) codegen modes.
#[cfg(any(feature = "alloc", feature = "std"))]
pub mod heap {
    pub use alloc::{string::String, vec::Vec};
}
