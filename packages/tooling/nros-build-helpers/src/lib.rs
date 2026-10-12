pub mod c;
pub mod codegen_version;
pub mod cpp;

mod shared;

// Issue 0452 — the committed cbindgen headers have exactly one writer, the
// `nros-cbindgen-headers` binary (`just regen-c-headers`). Build scripts reach
// only the comparison path inside `c`/`cpp`.
//
// phase-400 W2a — rendering needs cbindgen itself, so it is behind the
// `cbindgen-drift-check` feature that the regenerator enables. `write_committed_header`
// takes FINAL content and is cbindgen-free, so it stays unconditional.
#[cfg(feature = "cbindgen-drift-check")]
pub use shared::render_cbindgen_header;
pub use shared::write_committed_header;

// Issue 1852 — the per-build header directory names, for the `nros-c` /
// `nros-cpp` build scripts that publish them on their `links` channel. One
// definition: `nros_build_paths::artifact`.
pub use nros_build_paths::artifact::{C_GENERATED_DIR, CPP_GENERATED_DIR};
