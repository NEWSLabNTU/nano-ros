//! Entry pkg for the parameterised Rust workspace on Zephyr.
//!
//! phase-276 W1 (#102 H1, issue #128) — the params-on-embedded projection of
//! `ws-params-rust`. Same one-line `nros::main!(launch = ...)` as the native
//! sibling; `deploy = "zephyr"` (Cargo.toml) routes it onto the
//! `Framework::Zephyr` emit branch, which (post-#128) also carries the
//! capability emits:
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. parses its `system.launch.xml` + `system.toml` — the `<param>`
//!      initials are compile-baked; `[param_services]` arms the emit,
//!   3. `apply_param_services(&[("param_talker.publish_period_ms", "250")])`
//!      BEFORE the register call (the store must exist when the node's cell
//!      captures it), then `param_talker_pkg::register(runtime)?;`,
//!   4. exports `rust_main` that gates on the network, opens an `Executor`,
//!      registers, and spins forever — `ros2 param get/set` reaches the six
//!      parameter services over the zenoh session.
//!
//! There is NO Rust `fn main` (Zephyr emits the C `main`).

#![no_std]

// Zephyr's allocator + panic + boot belong to the RTOS; pull the crate
// in so the kernel's Rust glue (`set_logger`, allocator hookup) links.
extern crate zephyr;

nros::main!(model = "demo_bringup");
