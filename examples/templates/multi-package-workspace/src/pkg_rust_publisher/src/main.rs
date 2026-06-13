//! Multi-package-workspace demo — Rust publisher Entry pkg.
//!
//! `nros::main!()` (Form-1 self-bringup) reads
//! `[package.metadata.nros.entry] deploy = "native"` from this pkg's
//! `Cargo.toml`, maps the deploy key to `nros_board_native::NativeBoard`,
//! and emits the host boot scaffold: it brings up the board, opens the
//! executor, registers this pkg's `Talker` node (its sibling `lib.rs`
//! `nros::node!` export) and spins. The application logic lives in
//! `src/lib.rs`.

nros::main!();
