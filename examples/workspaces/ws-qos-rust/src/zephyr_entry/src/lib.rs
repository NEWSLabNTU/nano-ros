//! Entry pkg for the QoS-override Rust workspace on Zephyr.
//!
//! phase-276 W5 (#102 H1) — the qos-on-embedded projection of `ws-qos-rust`.
//! Same one-line `nros::main!(launch = ...)` as the native sibling;
//! `deploy = "zephyr"` (Cargo.toml) routes it onto the `Framework::Zephyr`
//! emit branch (plain register+spin — the QoS profiles are declared
//! per-entity in node code, RFC-0041):
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. parses its `system.launch.xml` (reliable_talker + qos_listener),
//!   3. `reliable_talker_pkg::register(runtime)?;` +
//!      `qos_listener_pkg::register(runtime)?;` — the talker publishes
//!      `/qos_chatter` with a NON-DEFAULT profile (reliable + transient_local)
//!      and the listener subscribes with the byte-identical profile,
//!   4. exports `rust_main` that gates on the network, opens an `Executor`,
//!      registers, and spins forever — the on-target QoS-matched pair
//!      republishes its receive count on `/qos_ok` for cross-process observers.
//!
//! There is NO Rust `fn main` (Zephyr emits the C `main`).

#![no_std]

// Zephyr's allocator + panic + boot belong to the RTOS; pull the crate
// in so the kernel's Rust glue (`set_logger`, allocator hookup) links.
extern crate zephyr;

nros::main!(model = "demo_bringup");
