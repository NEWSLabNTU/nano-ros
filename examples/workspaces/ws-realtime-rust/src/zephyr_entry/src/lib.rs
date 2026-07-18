//! Entry pkg for the RT-tiers Rust workspace on Zephyr.
//!
//! phase-276 W2 (#102 H1, issue #128 half 2) — the tiers-on-embedded
//! projection of `ws-realtime-rust`. Same one-line `nros::main!(model = ...)`
//! as the native sibling; `deploy = "zephyr"` (Cargo.toml) routes it onto the
//! `Framework::Zephyr` emit branch, and the `[tiers.*]` table in `system.toml`
//! (with `[tiers.*.zephyr]` raw priorities) flips that arm onto
//! `ZephyrBoard::run_tiers` (RFC-0015 Model 1):
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. parses its `system.launch.xml` (ctrl + telem) + `system.toml` tiers,
//!   3. spawns one `k_thread` per tier over ONE shared zenoh session — the
//!      boot thread runs the `high` tier (`ctrl`, 10 ms), a pool thread runs
//!      `low` (`telem`, 100 ms); each tier registers through the same closure
//!      with its `active_groups` filter installed,
//!   4. the nodes publish `/ctrl` + `/telem` for cross-process observers.
//!
//! There is NO Rust `fn main` (Zephyr emits the C `main`).

#![no_std]

// Zephyr's allocator + panic + boot belong to the RTOS; pull the crate
// in so the kernel's Rust glue (`set_logger`, allocator hookup) links.
extern crate zephyr;

// RFC-0052 / phase-296 R2 — canonical model bake path (see native_entry).
nros::main!(model = "demo_bringup");
