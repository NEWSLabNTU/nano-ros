//! Rust mixed-workspace consumer — Entry pkg.
//!
//! `nros::main!()` (Form-1 self-bringup) reads
//! `[image.native] board = "native"` from the `system.toml` beside this
//! pkg's `Cargo.toml`, maps the board to `nros_board_linux::LinuxBoard`,
//! and emits the host boot scaffold: it brings up the board, opens the
//! executor, registers this pkg's `Consumer` node (its sibling `lib.rs`
//! `nros::node!` export) and spins. The application logic — importing
//! msgs from both the workspace and AMENT — lives in `src/lib.rs`.
//!
//! How to build and run it — including whether a router has to be up, which
//! depends on the `rmw` this workspace declares and not on this source — is in
//! the template's README, under "Build — Rust".

nros::main!();
