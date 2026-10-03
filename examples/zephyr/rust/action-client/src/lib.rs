//! Zephyr Fibonacci action client.
//!
//! Declarative: node + action client.
//!
//! One-shot `send_goal` on the first `tick`; feedback and the terminal
//! result are delivered to `on_callback` (`on_feedback` / `on_result`).
//!
//! Issue 1603 -- the node itself is `zephyr_action_client_node` (`node/`), a
//! package with no Zephyr dependency, so the host metadata probe can build it.
//! This crate is the IMAGE half: Zephyr links it as a staticlib, and
//! `app_main` holds the boot glue.

#![no_std]

mod app_main;

pub use zephyr_action_client_node::FibonacciClient;
