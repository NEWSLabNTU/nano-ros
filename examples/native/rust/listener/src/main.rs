//! Native Listener entry — the generated boot scaffold for `lib.rs`.
//!
//! `spin = "forever"` (issue 0274) is what the imperative version did with
//! `executor.spin_blocking(SpinOptions::default())`.

nros::main!(spin = "forever");
