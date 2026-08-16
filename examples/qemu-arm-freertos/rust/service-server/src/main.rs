//! Phase 213.C.1 — entry (collapsed into the role package, phase-338 W2), N.9 macro shape.
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

// phase-366 W5.c — this image's ending, declared by the image. Forwards to
// `nros_platform_panic`, which on this board is semihosting + `bkpt` + halt
// (`nros-platform-mps2-an385`'s `PlatformPanic`). Swap for
// `use panic_semihosting as _;` with `features = ["exit"]` if you want a panic
// to end the QEMU run, as the logging smoke fixture does.
nros::panic_to_platform!();

nros::main!();
