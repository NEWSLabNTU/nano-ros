//! Phase 213.C.1 — entry (collapsed into the role package, phase-338 W2), N.9 macro shape.
//!
//! `nros::main!()` reads the board (`[image.*] board = "freertos"`) and the
//! network identity from this pkg's `system.toml` (RFC-0098 D3/D5), maps
//! `"freertos"` → `::nros_board_mps2_an385_freertos::Mps2An385`, and emits
//! the full `fn main()` body that delegates to `<Mps2An385 as
//! BoardEntry>::run_with_deploy`.
//!
//! Replaces the legacy `build.rs + include!()` codegen-stub shape
//! end-to-end (see Phase 213.C.1 in the post-212 known-issues doc).

#![no_std]
#![no_main]

nros::main!(panic = "platform");
