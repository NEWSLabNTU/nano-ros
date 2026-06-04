//! Entry pkg — boots the `demo_bringup` topology against the native
//! board.
//!
//! The body collapses to the one-line `nros::main!()` macro. The
//! `launch = "demo_bringup:system.launch.xml"` form makes the macro:
//!   1. read `[package.metadata.nros.entry] deploy = "native"` →
//!      `nros_board_native::NativeBoard`,
//!   2. resolve `demo_bringup` via the workspace pkg-index,
//!   3. parse `demo_bringup/launch/system.launch.xml`,
//!   4. emit `talker_pkg::register(runtime)?;` +
//!      `listener_pkg::register(runtime)?;`, then drive the board's
//!      executor + spin loop.
//!
//! Drop the `:system.launch.xml` suffix to use the bringup's
//! `system.toml::[system].default_launch` instead.

nros::main!(launch = "demo_bringup:system.launch.xml");
