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

// phase-359 W2 (final) / issue 0581 — `any(alloc, std)` is THE heap predicate,
// used in this exact spelling everywhere in the workspace.
//
// `alloc` and `std` are standard-library CRATES, not Cargo features; the
// features are only our convention for "compile the heap paths". A `std` build
// links an allocator by definition, so it has a heap — hence `any(...)`, and
// hence a hosted consumer never has to name `alloc`. Verified: a `no_std` rlib
// using `alloc` builds for thumbv7em with no allocator anywhere; the
// requirement appears only when a final artifact is linked
// ("no global memory allocator found but one is required"), which is exactly
// where malloc is unified per platform.
//
// Issue 0581 was never this predicate — it was that THIS crate used
// `any(alloc, std)` while `nros-serdes` used `alloc` alone, so a `std`-only
// build got a `heap::Vec<T>` it could name and could not serialize. The fix is
// that both now spell it the same way.
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
    CdrReader, CdrWriter, DHeaderMark, DHeaderScope, DeserError, Deserialize, DeserializeBorrowed,
    EncodingVersion, LeDecode, LeSliceView, SerError, Serialize,
};

// Re-export heapless for generated message types
pub use heapless;

/// Heap-backed containers for generated `mode = "heap"` message fields
/// (RFC-0033). Available whenever a heap is — the `alloc` feature, or `std`,
/// which links an allocator by definition. `nros-serdes` gates its matching
/// `Serialize`/`Deserialize` impls on the SAME predicate, which is what issue
/// 0581 was about. Generated code
/// refers to `nros_core::heap::{Vec, String}` so the same path works in both
/// crate and inline (`build.rs`) codegen modes.
#[cfg(any(feature = "alloc", feature = "std"))]
pub mod heap {
    pub use alloc::{string::String, vec::Vec};
}
