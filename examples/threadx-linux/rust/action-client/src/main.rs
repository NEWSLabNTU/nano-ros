//! Phase 213.C.3 — N.9 macro shape.
//!
//! `nros::main!()` reads the board
//! (`[image.*] board = "threadx-linux"`) from this pkg's `system.toml`
//! (RFC-0098 D3), maps `"threadx-linux"` →
//! `::nros_board_threadx_linux::ThreadxLinux`, and emits a C-ABI `main`
//! that delegates to `<ThreadxLinux as BoardEntry>::run(...)`. The
//! sibling Node pkg `threadx_linux_rs_action_client` is linked via the
//! `[dependencies]` block; its `register` symbol is the macro's
//! dispatch target.
//!
//! issue 1759 — `#![no_std]` + `#![no_main]`: glibc's crt0 calls the C-ABI
//! `main(argc, argv)` the macro emits (the board's `entry_kind` is
//! `board-run`), a panic ends in `nros_platform_panic`, and `alloc` is served by
//! the ThreadX byte pool. No libstd is linked.

#![no_std]
#![no_main]

nros::main!(panic = "platform");
