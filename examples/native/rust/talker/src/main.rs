//! Native Talker entry — the generated boot scaffold for `lib.rs`'s Node.
//!
//! Platform glue lives here; node logic lives in `lib.rs` and is
//! byte-identical to every other scheduled-platform copy (phase-338 W1's
//! `example_portability` gate asserts it).
//!
//! `spin = "forever"` (issue 0274) is what the imperative version did with
//! `executor.spin_blocking(SpinOptions::default())`. Without it the generated
//! hosted main uses the env-gated BOUNDED spin and the process exits at once,
//! having published nothing.

nros::main!(spin = "forever");
