//! Zephyr Fibonacci action server.
//!
//! Declarative: node + action server with distinct goal / cancel /
//! accepted callbacks. Bodies:
//!  - `on_goal` accepts non-negative orders, rejects otherwise.
//!  - `on_cancel` always accepts.
//!  - `on_accepted` is a no-op (the per-spin work runs in `tick()`).
//!  - `tick()` walks every active goal, publishes feedback, completes.
//!
//! Issue 1603 -- the node itself is `zephyr_action_server_node` (`node/`), a
//! package with no Zephyr dependency, so the host metadata probe can build it.
//! This crate is the IMAGE half: Zephyr links it as a staticlib, and
//! `app_main` holds the boot glue.

#![no_std]

mod app_main;

pub use zephyr_action_server_node::FibonacciServer;
