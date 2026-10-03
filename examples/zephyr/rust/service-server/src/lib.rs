//! Zephyr AddTwoInts service server.
//!
//! Declarative: node + service server with a `handle_add` callback.
//! Body: reads the typed request, writes the typed reply through the
//! reply sink. Generated runtime owns init / executor / spin.
//!
//! Issue 1603 -- the node itself is `zephyr_rs_service_server_node` (`node/`), a
//! package with no Zephyr dependency, so the host metadata probe can build it.
//! This crate is the IMAGE half: Zephyr links it as a staticlib, and
//! `app_main` holds the boot glue.

#![no_std]

mod app_main;

pub use zephyr_rs_service_server_node::AddTwoIntsServer;
