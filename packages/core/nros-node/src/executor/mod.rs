//! Embedded executor — backend-agnostic via `Session` trait.
//!
//! Provides [`Executor<S>`] and [`Node<S>`] that work with any
//! [`Session`](nros_rmw::Session) implementation (zenoh, XRCE-DDS, or third-party backends).
//!
//! # Example
//!
//! ```ignore
//! use nros_node::executor::*;
//! use std_msgs::msg::Int32;
//!
//! // Any Session implementation works:
//! let session = MyBackend::open(&config)?;
//! let mut executor = Executor::from_session(session);
//! let mut node = executor.create_node("my_node")?;
//!
//! let publisher = node.create_publisher::<Int32>("/chatter")?;
//! publisher.publish(&Int32 { data: 42 })?;
//!
//! loop {
//!     executor.spin_once(10);
//! }
//! ```

pub mod action_core;
mod arena;
mod handles;
mod node;
mod spin;
mod types;

pub mod action;

#[cfg(test)]
mod tests;

// Flat re-exports so users write `executor::Executor` etc.
pub use action::{ActionServerHandle, ActionServerRawHandle};
pub use action_core::{ActionClientCore, ActionServerCore, RawActiveGoal};
pub use handles::*;
pub use node::Node;
pub use spin::Executor;
pub use types::*;
