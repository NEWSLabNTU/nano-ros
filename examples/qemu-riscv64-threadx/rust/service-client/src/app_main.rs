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

nros_board_threadx_qemu_riscv64::app_main!(crate::register);

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

// phase-366 W5.c — this image's ending, declared where the image is built.
//
// In the GLUE module, not `lib.rs`: it must be in the lib's module tree (the
// `staticlib` is a FINAL ARTIFACT and needs the lang item on its own account —
// RFC-0077's staticlib qualification, which is also why it cannot live in
// `main.rs`), and `lib.rs` is node LOGIC, compared byte-for-byte against every
// other scheduled-platform copy by `example_portability`. A `#[panic_handler]`
// is boot glue in exactly the sense this file's header describes, so putting it
// here satisfies both: the image still declares its own ending, visibly, and
// the node body stays portable. Declaring it in `lib.rs` diverged all six
// copies from `native` (2026-08-16).
//
// Swap for `use panic_halt as _;`, or write a handler that logs and reboots.
nros::panic_to_platform!();
