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
