//! Phase 213.C.1 — `freertos_rs_listener_entry` Entry pkg, N.9 macro shape.
//!
//! `nros::main!()` reads `[package.metadata.nros.entry] deploy = "freertos"`
//! from this pkg's `Cargo.toml`, maps `"freertos"` →
//! `::nros_board_mps2_an385_freertos::Mps2An385`, walks the sibling
//! `launch/system.launch.xml` (empty in this step), and emits the full
//! `fn main()` body that delegates to `<Mps2An385 as BoardEntry>::run`.
//!
//! Replaces the legacy `build.rs + include!()` codegen-stub shape
//! end-to-end (see Phase 213.C.1 in the post-212 known-issues doc).

#![no_std]
#![no_main]

nros::main!();
