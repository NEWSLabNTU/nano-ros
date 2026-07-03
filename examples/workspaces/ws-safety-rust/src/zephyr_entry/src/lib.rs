//! Entry pkg for the E2E-safety (CRC) Rust workspace on Zephyr.
//!
//! phase-276 W4 (#102 H1) — the safety-on-embedded projection of
//! `ws-safety-rust`. Same one-line `nros::main!(launch = ...)` as the native
//! sibling; `deploy = "zephyr"` (Cargo.toml) routes it onto the
//! `Framework::Zephyr` emit branch (plain register+spin — the safety
//! capability rides the `safety-e2e` backend feature, not a capability emit):
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. parses its `system.launch.xml` (talker + safe_listener),
//!   3. `talker_pkg::register(runtime)?;` + `safe_listener_pkg::register(runtime)?;`
//!      — the backend attaches the E2E CRC + sequence number on publish and
//!      validates on receive; the listener reads `CallbackCtx::integrity()`,
//!   4. exports `rust_main` that gates on the network, opens an `Executor`,
//!      registers, and spins forever — the listener republishes its
//!      CRC-VALIDATED receive count on `/safe_ok` for cross-process observers.
//!
//! There is NO Rust `fn main` (Zephyr emits the C `main`).

#![no_std]

// Zephyr's allocator + panic + boot belong to the RTOS; pull the crate
// in so the kernel's Rust glue (`set_logger`, allocator hookup) links.
extern crate zephyr;

nros::main!(launch = "demo_bringup:system.launch.xml");
