//! Zephyr Talker — Phase 212.M.3 / Phase 212.L Node pkg.
//!
//! Publishes `std_msgs/String` (`Hello World: N`) on `/chatter` once per
//! second, matching the official ROS 2 `demo_nodes_cpp` talker.
//!
//! Node pkg shape: `register()` declares node + publisher + timer;
//! `ExecutableNode::on_callback("on_tick")` runs the timer body
//! (bump counter, publish). `nros::zephyr_component_main!(Talker)`
//! owns executor open, node registration, and the spin loop for this
//! self-package Rust application.
//! The user authors *only* the declarative + body bits.
//!
//! RMW selection is the IMAGE's `rmw` in `system.toml` (one image per RMW,
//! picked with `-DNROS_IMAGE=<id>`); the nano-ros Zephyr module renders it
//! into Kconfig `CONFIG_NROS_RMW_*` (phase-481, RFC-0098 D11), and the
//! example `CMakeLists.txt` threads that choice into Cargo feature
//! selection.
//!
//! Issue 1603 -- the node itself is `zephyr_talker_node` (`node/`), a
//! package with no Zephyr dependency, so the host metadata probe can build it.
//! This crate is the IMAGE half: Zephyr links it as a staticlib, and
//! `app_main` holds the boot glue.

#![no_std]

mod app_main;

pub use zephyr_talker_node::Talker;
