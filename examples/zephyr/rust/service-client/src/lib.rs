//! Zephyr AddTwoInts service client.
//!
//! Declarative metadata: node + service client + driver timer.
//!
//! The timer fires → `on_callback` flips the state's `pending` flag. Real
//! call dispatch lives in `tick` (the only place `&mut Executor` is free —
//! see `TickCtx` docs). Sends ONE fixed request (2, 3); the timer retries
//! until the call succeeds (discovery warm-up), then goes quiet.
//!
//! Issue 1603 -- the node itself is `zephyr_rs_service_client_node` (`node/`), a
//! package with no Zephyr dependency, so the host metadata probe can build it.
//! This crate is the IMAGE half: Zephyr links it as a staticlib, and
//! `app_main` holds the boot glue.

#![no_std]

mod app_main;

pub use zephyr_rs_service_client_node::AddTwoIntsClient;
