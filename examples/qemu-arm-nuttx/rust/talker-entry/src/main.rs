//! Phase 213.C.2 — N.9 macro shape.
//!
//! Entry pkg for the NuttX QEMU ARM talker. `nros::main!()` reads
//! `[package.metadata.nros.entry] deploy = "nuttx"` from this pkg's
//! `Cargo.toml`, maps `"nuttx"` → `::nros_board_nuttx_qemu_arm::QemuArmVirt`,
//! and emits `fn main()` that delegates to `<QemuArmVirt as BoardEntry>::run(...)`.
//!
//! Replaces the legacy `build.rs + include!(env!("OUT_DIR")/run_plan.rs)`
//! shape end-to-end.

nros::main!();
