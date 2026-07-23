//! Entry pkg for the shared Rust workspace on Zephyr.
//!
//! Phase 225.P — on Zephyr the RTOS framework *is* the workflow:
//! `west build` is the build verb, Kconfig (`prj-<rmw>.conf`) selects
//! the RMW, `west build -b <board>` picks the board, and nano-ros
//! integrates as a Zephyr module. This crate is a `staticlib` exporting
//! `rust_main`, which `zephyr-lang-rust`'s `rust_cargo_application()`
//! invokes after kernel + net init.
//!
//! The body is the SAME one-line `nros::main!(model = ...)` the
//! native / freertos / threadx entries use. `[package.metadata.nros.entry]
//! deploy = "zephyr"` routes the macro onto its `Framework::Zephyr` emit
//! branch, which:
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. parses `demo_bringup/launch/system.launch.xml`,
//!   3. emits `talker_pkg::register(runtime)?;` +
//!      `listener_pkg::register(runtime)?;` (launch file = single source
//!      of truth for the node set),
//!   4. exports `#[unsafe(no_mangle)] pub extern "C" fn rust_main()` that
//!      gates on `ZephyrBoard::wait_link_up`, opens an `Executor`, wraps
//!      it in `ExecutorNodeRuntime`, registers each node, and spins —
//!      bounded on hosted native_sim, forever on real hardware.
//!
//! There is NO Rust `fn main` (Zephyr emits the C `main`).

#![no_std]

// Zephyr's allocator + panic + boot belong to the RTOS; pull the crate
// in so the kernel's Rust glue (`set_logger`, allocator hookup) links.
extern crate zephyr;

nros::main!(model = "demo_bringup:config/system_model.yaml");
