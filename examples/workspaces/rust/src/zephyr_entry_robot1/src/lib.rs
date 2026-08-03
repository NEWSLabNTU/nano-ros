//! Per-host Entry pkg (`robot1`) for the Rust workspace on Zephyr.
//!
//! phase-276 W6 (#102 H1) — MULTIHOST on embedded: this Entry bakes ONLY the
//! `robot1` slice of `demo_bringup/launch/multihost.launch.xml` — the talker
//! — by consuming the per-host `multihost_robot1_model.yaml` (resolved with
//! `host:=robot1`; phase-326 / issue 0364 moved the partition to resolve
//! time). The listener is hosted on `robot2` (a native per-host entry in the
//! paired e2e), so the `/chatter` delivery crosses hosts: Zephyr native_sim
//! image → zenohd → native process.
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

nros::main!(launch = "demo_bringup:multihost.launch.xml", args = [("host", "robot1")]);
