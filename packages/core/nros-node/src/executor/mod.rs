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
mod spin;
#[cfg(any(has_rmw, test))]
pub(crate) mod spsc_ring;
#[cfg(any(has_rmw, test))]
pub(crate) mod triple_buffer;
mod types;

#[cfg(any(has_rmw, test))]
pub mod action;

#[cfg(test)]
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
