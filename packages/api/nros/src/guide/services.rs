//! # Service Calls and Promise API
//!
//! `client.call(&request)` returns a `Promise` immediately — no blocking.
//! The reply is polled with `take()` while the executor spins.
//!
//! ## Sync polling (no async runtime)
//!
//! Drive I/O with `spin_once()` while polling the promise:
//!
//! ```ignore
//! use nros::prelude::*;
//! use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
//!
//! let config = ExecutorConfig::from_env().node_name("client");
//! let mut executor: Executor = Executor::open(&config)?;
//! let mut node = executor.create_node("client")?;
//! let mut client = node.create_client::<AddTwoInts>("/add_two_ints")?;
//!
//! let mut promise = client.call(&AddTwoIntsRequest { a: 1, b: 2 })?;
//!
//! let reply = loop {
//!     executor.spin_once(core::time::Duration::from_millis(10));
//!     if let Ok(Some(reply)) = promise.take() {
//!         break reply;
//!     }
//! };
//! println!("sum = {}", reply.sum);
//! ```
//!
//! A promise is also a `Future`, but nano-ros drives no foreign async runtime:
//! an async task that waited on one with no spin running elsewhere would wait
//! forever. RFC-0113 retired `spin_async` because a host's scheduler, not
//! nano-ros, would decide when its callbacks run.
