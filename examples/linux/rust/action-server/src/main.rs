//! Native Fibonacci action-server entry — the boot scaffold for `lib.rs`.
//!
//! `spin = "forever"` (issue 0274) matches the imperative version's spin loop;
//! without it the generated hosted main uses the env-gated BOUNDED spin and the
//! server exits before a client can reach it.

nros::main!(spin = "forever");
