//! RFC-0089 / phase-429 W1 — the codegen version this generator stamps into
//! every artifact it emits, read from its ONE source of truth.
//!
//! The constants live in `packages/core/nros-core/src/codegen_version.rs` and
//! are `include!`d verbatim rather than reached through a dependency edge.
//! `packages/cli` is a separate workspace, so a `nros-core` path dep would add
//! rows to its lockfile for a pair of `const`s; `include!` costs nothing in the
//! dependency graph, and rustc records the included path in the depfile so an
//! edit to the constants rebuilds this crate.
//!
//! `nros-build-helpers` — which defines the matching runtime anchors — reaches
//! the same file the same way. Keep the two spellings identical.

include!("../../../core/nros-core/src/codegen_version.rs");
