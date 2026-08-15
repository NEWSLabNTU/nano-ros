//! phase-338 W2 — platform boot glue for the ThreadX RV64 staticlib path.
//!
//! Isolated from `lib.rs` so the node logic there is byte-identical to every
//! other scheduled-platform copy (the `example_portability` gate compares logic
//! files and ignores glue).
//!
//! This cannot live in `src/main.rs`: the CycloneDDS/CMake path links the
//! **staticlib**, which is built from `lib.rs`'s module tree, and a
//! `#[no_mangle]` symbol defined in the `main.rs` bin target never reaches it.
//! So the glue is a module of the lib, not a second crate root.

extern crate alloc;

// rustc's staticlib DCE drops a dependency's `#[no_mangle]` exports without a
// direct reference — the board's `register()` would be present in the rlib and
// absent from the `.a`. This anchor keeps it.
extern crate nros_board_threadx_qemu_riscv64 as _;

nros_board_threadx_qemu_riscv64::cyclonedds_app_main!(crate::register);

// phase-361 W8.e / issue 0594 — the backend REQUIRES the heap; it does not
// silently enable it. Build as `--features rmw-cyclonedds,alloc` (the cmake
// path spells both in `nros_threadx_rv64_rust_cyclone_app`).
//
// It lives in this glue module, not in `lib.rs`, for the reason stated at the
// top of this file: `lib.rs` is LOGIC and `example_portability` requires it to
// be byte-identical to every other scheduled-platform copy. Native needs no
// such guard — it is a `std` build, where `std` implies `alloc` — so putting a
// platform-specific feature assertion in the shared logic file diverges it from
// the group. Glue is exactly where a platform-specific assertion belongs, and
// `mod app_main;` is unconditional, so the guard is always evaluated.
#[cfg(all(feature = "rmw-cyclonedds", not(feature = "alloc")))]
compile_error!("`rmw-cyclonedds` allocates: add \"alloc\" (--features rmw-cyclonedds,alloc)");
