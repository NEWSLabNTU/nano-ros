//! Phase 213.C.3 — N.9 macro shape.
//!
//! `nros::main!()` (no args) reads the board
//! (`[image.*] board = "threadx-linux"`) from this pkg's `system.toml`
//! (RFC-0098 D3), maps `"threadx-linux"` →
//! `::nros_board_threadx_linux::ThreadxLinux`, and emits `fn main()`
//! that delegates to `<ThreadxLinux as BoardEntry>::run(...)`. The
//! sibling Node pkg `threadx_linux_rs_action_client` is linked via the
//! `[dependencies]` block; its `register` symbol is the macro's
//! dispatch target.

nros::main!();
