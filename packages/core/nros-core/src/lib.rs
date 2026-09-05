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

// phase-361 W2 (final) / issue 0598 — `alloc` is THE heap predicate, in this
// exact spelling everywhere in the workspace. `std` reaches it through the
// MANIFEST (`std = ["alloc", …]`), not through a wider `cfg`.
//
// A `std` build links an allocator by definition, so it does have a heap, and a
// hosted consumer must not have to name `alloc` — but that implication belongs
// in one manifest line per crate, not in every `cfg`. Spelling it
// `any(alloc, std)` at the use sites was tried and reverted: it put the same
// fact in 123 places and left phase-359 (which DELETES `std` from these crates)
// with 88 extra branches to unwind, in `nros-node` above all. With the manifest
// edge, dropping `std` needs no `cfg` edit at all — these gates are already in
// their final form.
//
// Issue 0598 was never about which predicate: it was that THIS crate's heap
// gate and `nros-serdes`'s disagreed, so a `std`-only build got a `heap::Vec<T>`
// it could name and could not serialize. The fix is that `std` now forwards
// `alloc` here, so the types and their impls arrive together.
#[cfg(feature = "alloc")]
extern crate alloc;

pub mod action;
pub mod clock;
// RFC-0090 / phase-429 — the generated-code <-> runtime compatibility token.
pub mod codegen_version;
// issue 0783 — there is no `error` module here any more, and its absence is the
// decision. It held `NanoRosError { code: RclReturnCode, context, nested }`, a
// phase-16 rclrs-shaped error, plus `RclReturnCode` (an `rcl_ret_t` numeric
// mirror), `ErrorContext`, `NestedError`, `NanoRosErrorFilter` and
// `TakeFailedAsNone`. Phase 84.D1 settled `NodeError` (nros-node) as the single
// user-facing error and deferred "folding NanoRosError into NodeError"; the fold
// never happened and nothing ever called the type. It was reachable from no
// public API: the `nros` facade never re-exported it, and this crate's own
// `RosAction::register_protocol_types` returns `Result<(), ()>` with a comment
// saying it cannot name an error type — with `NanoRosError` sitting in the same
// crate. RFC-0036's Errors row described it as the Rust user error for two
// years, which is the cost this deletion removes.
pub mod lifecycle;
pub mod logger;
pub mod message_info;
pub mod service;
pub mod time;
pub mod types;

pub use action::{
    ActionClient, ActionServer, CancelResponse, CancelReturnCode, GoalId, GoalInfo, GoalResponse,
    GoalStatus, GoalStatusStamped, RosAction,
};
pub use clock::{Clock, ClockType};
// RFC-0090 — generated code names these directly, so they are re-exported at
// the root: an emitted `const` assertion should not have to spell a module path
// that could be reorganised under it.
pub use codegen_version::{NROS_CODEGEN_VERSION, NROS_CODEGEN_VERSION_MIN};
pub use lifecycle::{LifecycleState, LifecycleTransition, TransitionResult};
pub use logger::{Logger, OnceFlag};
pub use message_info::{MessageInfo, PUBLISHER_GID_SIZE, RawMessageInfo};
pub use service::{ServiceCallback, ServiceClient, ServiceRequest, ServiceServer};
pub use time::{Duration, Time};
pub use types::{RosMessage, RosService, ViewableMessage};

// Re-export serdes types for convenience
pub use nros_serdes::{
    CdrReader, CdrWriter, DHeaderMark, DHeaderScope, DeserError, Deserialize, DeserializeView,
    EncodingVersion, LeDecode, LeSliceView, SerError, Serialize,
};

// Re-export heapless for generated message types
pub use heapless;

/// Heap-backed containers for generated `mode = "heap"` message fields
/// (RFC-0033). Gated on `alloc`, which `std` implies — so a hosted consumer
/// gets these by asking for `std` and never has to name `alloc`. `nros-serdes`
/// gates its matching `Serialize`/`Deserialize` impls on the SAME feature and
/// receives it through the same forward, which is what issue 0598 was about.
/// Generated code refers to `nros_core::heap::{Vec, String}` so the same path
/// works in both crate and inline (`build.rs`) codegen modes.
#[cfg(feature = "alloc")]
pub mod heap {
    pub use alloc::{string::String, vec::Vec};
}
