//! Per-host Entry pkg (`robot1`) for the Rust workspace on Zephyr.
//!
//! phase-276 W6 (#102 H1) — MULTIHOST on embedded: this Entry bakes ONLY the
//! `robot1` slice of `demo_bringup/launch/multihost.launch.xml` via the
//! macro's `host = "robot1"` filter (Phase 211.F) — the talker. The listener
//! is hosted on `robot2` (a native per-host entry in the paired e2e), so the
//! `/chatter` delivery crosses hosts: Zephyr native_sim image → zenohd →
//! native process.
//!
//! Same `staticlib` + `rust_main` + `Framework::Zephyr` emit shape as the
//! sibling `zephyr_entry`; `deploy = "zephyr"` (Cargo.toml) routes the macro,
//! and the west lane bakes the router locator via CONFIG_NROS_ZENOH_LOCATOR.
//!
//! There is NO Rust `fn main` (Zephyr emits the C `main`).

#![no_std]

// Zephyr's allocator + panic + boot belong to the RTOS; pull the crate
// in so the kernel's Rust glue (`set_logger`, allocator hookup) links.
extern crate zephyr;

nros::main!(
    model = "demo_bringup:config/multihost_model.yaml",
    host = "robot1"
);
