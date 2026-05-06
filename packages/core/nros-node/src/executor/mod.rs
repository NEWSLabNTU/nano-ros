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
mod arena;
#[cfg(any(has_rmw, test))]
mod handles;
#[cfg(any(has_rmw, test))]
mod node;
#[cfg(any(has_rmw, test))]
pub(crate) mod activator;
#[cfg(any(has_rmw, test))]
pub(crate) mod dispatcher;
#[cfg(any(has_rmw, test))]
pub(crate) mod ready_set;
#[cfg(any(has_rmw, test))]
pub mod sched_context;
#[cfg(any(has_rmw, test))]
mod spin;
#[cfg(any(has_rmw, test))]
pub(crate) mod spsc_ring;
#[cfg(any(has_rmw, test))]
pub(crate) mod triple_buffer;
mod types;

#[cfg(any(has_rmw, test))]
pub mod action;

// MockSession-based tests. Disabled when any rmw-* feature is active because
// feature unification under `cargo test --workspace` flips `ConcreteSession`
// to a real backend handle (e.g. UorbSession when rmw-uorb is on transitively
// via the workspace), breaking the type signatures the tests expect.
#[cfg(all(
    test,
    not(any(
        feature = "rmw-zenoh",
        feature = "rmw-xrce",
        feature = "rmw-dds",
        feature = "rmw-cffi",
        feature = "rmw-uorb"
    ))
))]
mod tests;

// Flat re-exports so users write `executor::Executor` etc.
#[cfg(any(has_rmw, test))]
pub use action::{ActionClientRawHandle, ActionServerHandle, ActionServerRawHandle};
#[cfg(any(has_rmw, test))]
pub use action_core::{ActionClientCore, ActionServerCore, RawActiveGoal};
#[cfg(any(has_rmw, test))]
pub use handles::*;
#[cfg(any(has_rmw, test))]
pub use node::Node;
#[cfg(any(has_rmw, test))]
pub use spin::Executor;
pub use types::*;
