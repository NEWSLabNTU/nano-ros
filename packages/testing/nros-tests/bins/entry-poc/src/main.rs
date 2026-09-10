//! Phase 212.N.9 — Entry pkg `main.rs` collapsed to one line.
//!
//! `nros::main!()` (no args) reads the board
//! (`[image.*] board = "native"`) from this pkg's `system.toml`
//! (RFC-0098 D3), maps `"native"` → `::nros_board_linux::LinuxBoard`,
//! and emits `fn main()` that delegates to
//! `<LinuxBoard as BoardEntry>::run(...)`. The setup closure
//! dispatches `::entry_poc::register(runtime)?;` (this pkg's
//! companion `lib.rs`).
//!
//! Replaces the legacy `build.rs + include!(env!("OUT_DIR")/run_plan.rs)`
//! shape end-to-end. The Entry pkg's whole boot path now sits in
//! one expression.

nros::main!();
