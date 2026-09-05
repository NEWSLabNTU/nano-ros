//! RFC-0090 / phase-429 W1 — the codegen version range, read from its ONE
//! source of truth.
//!
//! The constants live in `packages/core/nros-core/src/codegen_version.rs` and
//! are `include!`d here verbatim rather than reached through a dependency edge.
//! A `nros-core` dependency on this crate would land in every tracked leaf
//! lockfile that already contains `nros-build-helpers` (see the note in this
//! crate's manifest about `nros-cbindgen-headers`), and this crate is
//! deliberately host-only. `include!` costs nothing in the dependency graph and
//! still gives exactly one place where the range is written — and rustc records
//! the included path in the depfile, so an edit to the constants rebuilds this
//! crate.
//!
//! `rosidl-codegen`, in the separate `packages/cli` workspace, reaches the same
//! file the same way. Keep the two spellings identical.
//!
//! What this crate does with the range: writes it into the generated config
//! headers (`nros_config_generated.h`, `nros_cpp_config_generated.h`) as
//! `NROS_CODEGEN_VERSION` / `NROS_CODEGEN_VERSION_MIN`, which is what generated
//! C and C++ compare themselves against with the preprocessor. It used to also
//! define one weak anchor symbol per accepted version; that mechanism is gone
//! (RFC-0090, and `check-weak-symbols`).

include!("../../../core/nros-core/src/codegen_version.rs");
