//! Phase 212.N.9 fixture — Entry pkg `main.rs`.
//!
//! Each compile-check row of this template overlays this file with its own
//! `cases/<id>/src/demo_entry/src/main.rs.case` (issue 1656) — the four
//! `nros::main!()` forms, and the misuse verdicts:
//!
//! ```ignore
//! nros::main!();                                          // Form 1
//! nros::main!(board = ::nros_board_linux::LinuxBoard);  // Form 2
//! nros::main!(launch = "demo_bringup");                   // Form 3
//! nros::main!(
//!     board  = ::nros_board_linux::LinuxBoard,
//!     launch = "demo_bringup:sim.launch.xml",
//!     args   = [("use_sim", "true")],
//! );                                                       // Form 4
//! ```
//!
//! The default content (committed in the fixture) is form 3 — the
//! richest path that exercises pkg-index walk + launch.xml parse +
//! per-node register-call emit.

nros::main!(launch = "demo_bringup");
