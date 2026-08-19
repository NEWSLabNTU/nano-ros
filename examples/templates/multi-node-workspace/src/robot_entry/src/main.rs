//! Entry pkg — boots the `demo_bringup` topology against the native
//! board.
//!
//! The body collapses to the one-line `nros::main!()` macro. The
//! `launch = "demo_bringup"` form makes the macro:
//!   1. read `[package.metadata.nros.entry] deploy = "native"` →
//!      `nros_board_linux::LinuxBoard`,
//!   2. resolve `demo_bringup` via the workspace pkg-index,
//!   3. load the SystemModel `nros sync` resolved from the bringup's
//!      default launch file (a build artifact under `build/nros/models/`,
//!      never committed),
//!   4. emit `talker_pkg::register(runtime)?;` +
//!      `listener_pkg::register(runtime)?;`, then drive the board's
//!      executor + spin loop.
//!
//! Use `launch = "demo_bringup:<file>.launch.xml"` to pick a different
//! launch file from the same bringup.

// `spin = "forever"` keeps the process alive, ticking timers — the hosted
// default is a bounded spin gated on `NROS_ENTRY_SPIN_MS`, which registers
// everything and exits immediately when the env is absent (right for test
// fixtures, surprising for a first run).
nros::main!(launch = "demo_bringup", spin = "forever");
