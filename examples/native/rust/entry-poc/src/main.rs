//! Phase 212.N.9 — Entry pkg `main.rs` collapsed to one line.
//!
//! `nros::main!()` (no args) reads
//! `[package.metadata.nros.entry] deploy = "native"` from this
//! pkg's `Cargo.toml`, maps `"native"` → `::nros_board_native::NativeBoard`,
//! and emits `fn main()` that delegates to
//! `<NativeBoard as BoardEntry>::run(...)`. The setup closure
//! dispatches `::entry_poc::register(runtime)?;` (this pkg's
//! companion `lib.rs`).
//!
//! Replaces the legacy `build.rs + include!(env!("OUT_DIR")/run_plan.rs)`
//! shape end-to-end. The Entry pkg's whole boot path now sits in
//! one expression.

nros::main!();
