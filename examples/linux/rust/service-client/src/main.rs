//! Native AddTwoInts client entry — the boot scaffold for `lib.rs`.
//!
//! `spin = "forever"` (issue 0274): the client is tick-driven — it issues the
//! request from `tick()` — so it needs an unbounded spin.

nros::main!(spin = "forever");
