//! Phase 212.N.9 fixture — Entry pkg `main.rs`.
//!
//! The test harness overwrites this file with each of the four
//! `nros::main!()` forms in turn (then runs `cargo check`):
//!
//! ```ignore
//! nros::main!();                                          // Form 1
//! nros::main!(board = ::nros_board_native::NativeBoard);  // Form 2
//! nros::main!(model = "demo_bringup");                   // Form 3
//! nros::main!(
//!     board  = ::nros_board_native::NativeBoard,
//!     launch = "demo_bringup:sim.launch.xml",
//!     args   = [("use_sim", "true")],
//! );                                                       // Form 4
//! ```
//!
//! The default content (committed in the fixture) is form 3 — the
//! richest path that exercises pkg-index walk + launch.xml parse +
//! per-node register-call emit.

nros::main!(model = "demo_bringup");
