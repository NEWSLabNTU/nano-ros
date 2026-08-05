//! Native Fibonacci action-client entry — the boot scaffold for `lib.rs`.
//!
//! `spin = "forever"` (issue 0274): the client is tick-driven — it issues the
//! goal and polls feedback/result from `tick()` — so it needs an unbounded
//! spin, exactly as the imperative version's loop provided.

nros::main!(spin = "forever");
