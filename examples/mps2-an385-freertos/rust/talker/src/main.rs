//! Phase 213.C.1 — entry (collapsed into the role package, phase-338 W2), N.9 macro shape.
//!
//! `nros::main!()` reads the board (`[image.*] board = "freertos"`) and the
//! network identity from this pkg's `system.toml` (RFC-0098 D3/D5), maps
//! `"freertos"` → `nros_board_mps2_an385_freertos::Mps2An385` (reached through
//! the generated selection package, issue 1305), and emits
//! the full `fn main()` body that delegates to `<Mps2An385 as
//! BoardEntry>::run_with_deploy`.
//!
//! Replaces the legacy `build.rs + include!()` codegen-stub shape
//! end-to-end (see Phase 213.C.1 in the post-212 known-issues doc).

#![no_std]
#![no_main]

// The panic handler is the board's crate-root item (`use panic_semihosting as
// _;`), carried by the generated selection package with the board crate
// (issue 1305) -- the same pair a generated FreeRTOS workspace entry gets, whose
// image says `panic = "own"` for the same reason.
nros::main!(panic = "own");
