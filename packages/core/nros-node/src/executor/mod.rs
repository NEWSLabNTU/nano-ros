//! Embedded executor with build-time configured arena.
//!
//! Provides `Executor` and `Node` that work with the compile-time
//! selected RMW backend (zenoh, XRCE-DDS, or C FFI).
//!
//! # Example
//!
//! ```ignore
//! use nros_node::executor::*;
//! use std_msgs::msg::Int32;
//!
//! let config = ExecutorConfig::from_env().node_name("my_node");
//! let mut executor = Executor::open(&config)?;
//! let mut node = executor.create_node("my_node")?;
//!
//! let publisher = node.create_publisher::<Int32>("/chatter")?;
//! publisher.publish(&Int32 { data: 42 })?;
//!
//! loop {
//!     executor.spin_once(core::time::Duration::from_millis(10));
//! }
//! ```

#[cfg(any(has_rmw, test))]
pub mod action_core;
#[cfg(any(has_rmw, test))]
pub(crate) mod activator;
#[cfg(any(has_rmw, test))]
mod arena;
#[cfg(any(has_rmw, test))]
pub(crate) mod dispatcher;
#[cfg(any(has_rmw, test))]
mod handles;
#[cfg(feature = "std")]
pub mod handoff;
#[cfg(any(has_rmw, test))]
pub mod monitor;
#[cfg(any(has_rmw, test))]
mod node;
#[cfg(any(has_rmw, test))]
pub mod node_record;
#[cfg(any(has_rmw, test))]
mod node_wake;
// phase-359 W10 — the per-OS-priority worker pool, ported off `std::thread`
// onto the platform task ABI so it is reachable on every platform.
//
// Deliberately the SAME predicate as `node_wake` above, which it imports:
// spelling a second, hand-matched predicate here is how the two drift, and they
// did — an `all(feature = …, any(has_rmw, test))` copy resolved true in a build
// where `node_wake` resolved false. The feature half lives inside the file as
// an inner `#![cfg]`, so there is one condition per fact and no pair to keep in
// step.
#[cfg(any(has_rmw, test))]
pub(crate) mod os_priority;
// phase-359 W10 — the allocate/spawn/join helper moved to
// `nros_platform_api::task`, beside the ABI it wraps, once `nros-cpp` became a
// third caller. Reached through a `use` below rather than a module here.
#[cfg(any(has_rmw, test))]
pub(crate) mod ready_set;
pub mod sched_context;
#[cfg(any(has_rmw, test))]
mod spin;
#[cfg(any(has_rmw, test))]
pub(crate) mod spsc_ring;
#[cfg(any(has_rmw, test))]
mod storage;
#[cfg(any(has_rmw, test))]
pub(crate) mod triple_buffer;
mod types;
#[cfg(all(any(has_rmw, test), feature = "wake-latency-probe"))]
pub mod wake_probe;

#[cfg(any(has_rmw, test))]
pub mod action;

// MockSession-based tests. Disabled when any rmw-* feature is active because
// feature unification under `cargo test --workspace` flips `ConcreteSession`
// to a real backend handle (e.g. UorbSession when rmw-uorb is on transitively
// via the workspace), breaking the type signatures the tests expect.
#[cfg(all(test, not(feature = "rmw-cffi")))]
mod tests;

// Flat re-exports so users write `executor::Executor` etc.
#[cfg(any(has_rmw, test))]
pub use action::{
    ActionClientRawHandle, ActionServerHandle, ActionServerRawHandle, RawActionClientSpec,
    RawActionServerSpec,
};
#[cfg(any(has_rmw, test))]
pub use action_core::{ActionClientCore, ActionServerCore, RawActiveGoal, action_channel_type};
#[cfg(any(has_rmw, test))]
pub use arena::TimerOverrunPolicy;
#[cfg(any(has_rmw, test))]
pub use handles::*;
#[cfg(any(has_rmw, test))]
pub use node::{CallbackGroup, NodeHandle};
#[cfg(any(has_rmw, test))]
pub use node_record::{NodeBuilder, NodeId, NodeRecord};
#[cfg(any(has_rmw, test))]
pub use spin::Executor;
#[cfg(any(has_rmw, test))]
pub use spin::SessionHandle;
#[cfg(all(any(has_rmw, test), feature = "rmw-cffi"))]
pub use spin::SessionSpec;
#[cfg(any(has_rmw, test))]
pub use storage::{
    ExecutorInlineStorage, ExecutorSizing, executor_storage_layout, executor_storage_u64_len,
};
pub use types::*;

// issue 0687 — the one `$NROS_RMW` reader, reachable by the hosted callers
// (`nros`, `nros-c`) that used to each read the variable themselves.
pub use types::rmw_selector;
