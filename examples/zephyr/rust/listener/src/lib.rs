//! Zephyr Listener — Phase 212.M.3 / Phase 212.L Node pkg.
//!
//! Subscribes to `std_msgs/String` on `/chatter` and logs each message
//! (`I heard: [Hello World: N]`), matching the official ROS 2
//! `demo_nodes_cpp` listener. `nros::zephyr_component_main!(Listener)` owns
//! executor open, node registration, and the spin loop for this
//! self-package Rust application.
//!
//! Issue 1603 -- the node itself is `zephyr_listener_node` (`node/`), a
//! package with no Zephyr dependency, so the host metadata probe can build it.
//! This crate is the IMAGE half: Zephyr links it as a staticlib, and
//! `app_main` holds the boot glue.

#![no_std]

mod app_main;

pub use zephyr_listener_node::Listener;
