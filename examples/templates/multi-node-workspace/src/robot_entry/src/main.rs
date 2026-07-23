//! Entry pkg — boots the `demo_bringup` topology against the native
//! board.
//!
//! The body collapses to the one-line `nros::main!()` macro. The
//! `model = "demo_bringup"` form makes the macro:
//!   1. read `[package.metadata.nros.entry] deploy = "native"` →
//!      `nros_board_native::NativeBoard`,
//!   2. resolve `demo_bringup` via the workspace pkg-index,
//!   3. load the resolved SystemModel at
//!      `demo_bringup/config/system_model.yaml` (emitted by
//!      `play_launch resolve`),
//!   4. emit `talker_pkg::register(runtime)?;` +
//!      `listener_pkg::register(runtime)?;`, then drive the board's
//!      executor + spin loop.
//!
//! Use `model = "demo_bringup:config/<file>.yaml"` to pick a different
//! model file from the same bringup.

nros::main!(model = "demo_bringup");
