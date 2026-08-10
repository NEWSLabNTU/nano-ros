pub mod c;
pub mod cpp;

mod shared;

// Issue 0452 — the committed cbindgen headers have exactly one writer, the
// `nros-cbindgen-headers` binary (`just regen-c-headers`). Build scripts reach
// only the comparison path inside `c`/`cpp`.
pub use shared::{render_cbindgen_header, write_committed_header};
