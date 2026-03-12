//! # Service Calls and Promise API
//!
//! `client.call(&request)` returns a [`Promise`](crate::Promise) immediately — no blocking.
//! The reply can be polled with `try_recv()` or `.await`ed.
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
//!     executor.spin_once(10);
//!     if let Ok(Some(reply)) = promise.try_recv() {
//!         break reply;
//!     }
//! };
//! println!("sum = {}", reply.sum);
//! ```
//!
//! ## Async (background spin task)
//!
//! Spawn `spin_async()` as a background task, then `.await` promises:
//!
//! ```ignore
//! use nros::prelude::*;
//! use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
//!
//! #[tokio::main(flavor = "current_thread")]
//! async fn main() {
//!     let config = ExecutorConfig::from_env().node_name("client");
//!     let mut executor: Executor = Executor::open(&config).unwrap();
//!     let mut client = {
//!         let mut node = executor.create_node("client").unwrap();
//!         node.create_client::<AddTwoInts>("/add_two_ints").unwrap()
//!     };
//!
//!     let local = tokio::task::LocalSet::new();
//!     local.run_until(async move {
//!         tokio::task::spawn_local(async move {
//!             executor.spin_async().await;
//!         });
//!         let reply = client.call(&AddTwoIntsRequest { a: 1, b: 2 })
//!             .unwrap().await.unwrap();
//!         println!("sum = {}", reply.sum);
//!     }).await;
//! }
//! ```
//!
//! The Promise and `spin_async()` APIs use only `core::future` — no
//! external runtime dependency.  They work on `no_std`/`no_alloc` targets.
//! For async combinators (`select`, `join`), add `embassy-futures`.
//!
//! See `examples/native/rust/zenoh/async-service-client/` (tokio) and
//! `examples/zephyr/rust/zenoh/async-service-client/` (Embassy) for complete
//! working examples.
