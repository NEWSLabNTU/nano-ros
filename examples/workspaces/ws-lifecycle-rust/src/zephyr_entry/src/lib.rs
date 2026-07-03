//! Entry pkg for the managed (lifecycle) Rust workspace on Zephyr.
//!
//! phase-276 W3 (#102 H1, issue #128) — the lifecycle-on-embedded projection of
//! `ws-lifecycle-rust`. Same one-line `nros::main!(launch = ...)` as the native
//! sibling; `deploy = "zephyr"` (Cargo.toml) routes it onto the
//! `Framework::Zephyr` emit branch, which (post-#128) also carries the
//! capability emits:
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. parses its `system.launch.xml` + `system.toml` — `[lifecycle]
//!      autostart = "active"` arms the emit,
//!   3. `talker_pkg::register(runtime)?;` then `apply_lifecycle(...)` AFTER the
//!      registers — installs the 5 REP-2002 lifecycle services and drives the
//!      boot autostart (Configure → Activate),
//!   4. exports `rust_main` that gates on the network, opens an `Executor`,
//!      registers, and spins forever — `ros2 lifecycle nodes/get` reaches the
//!      managed node over the zenoh session and reports `active` at boot.
//!
//! There is NO Rust `fn main` (Zephyr emits the C `main`).

#![no_std]

// Zephyr's allocator + panic + boot belong to the RTOS; pull the crate
// in so the kernel's Rust glue (`set_logger`, allocator hookup) links.
extern crate zephyr;

nros::main!(launch = "demo_bringup:system.launch.xml");
