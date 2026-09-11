//! Multi-package-workspace demo — Rust publisher Entry pkg.
//!
//! `nros::main!()` (Form-1 self-bringup) reads
//! `[image.native] board = "native"` from the `system.toml` beside this
//! pkg's `Cargo.toml`, maps the board to `nros_board_linux::LinuxBoard`,
//! and emits the host boot scaffold: it brings up the board, opens the
//! executor, registers this pkg's `Talker` node (its sibling `lib.rs`
//! `nros::node!` export) and spins. The application logic lives in
//! `src/lib.rs`.

nros::main!();
