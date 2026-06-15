//! `nros-build-profile` — a passive, read-only build profiler.
//!
//! The build runs unchanged on its native toolchain (`west`, `cmake`, `idf.py`,
//! `cargo`); this crate only *reads* the timing artifacts the build already
//! emits and folds them into a normalized cross-backend [`model::BuildProfile`].
//! It never compiles or flashes anything.
//!
//! Pipeline: [`collect`] → `normalize` → `diagnostics` → `report`.
//! Design: `docs/superpowers/specs/2026-06-16-build-profiling-design.md`.

pub mod collect;
pub mod model;
