//! Native AddTwoInts server entry — the generated boot scaffold for `lib.rs`.
//!
//! `spin = "forever"` (issue 0274) is what the imperative version did with
//! `executor.spin_blocking(SpinOptions::default())`; without it the generated
//! hosted main uses the env-gated BOUNDED spin and the server exits before a
//! client can reach it.

nros::main!(spin = "forever");
