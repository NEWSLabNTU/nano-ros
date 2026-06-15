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
pub mod diagnostics;
pub mod model;
pub mod normalize;
pub mod report;

use std::path::Path;

/// Run both collectors over `dir` and fold them into a normalized profile.
/// Returns `None` when no timing artifacts were found at all (so the caller can
/// emit an actionable "nothing to profile" message instead of an empty table).
pub fn analyze(dir: &Path) -> Option<model::BuildProfile> {
    let collected: Vec<collect::Collected> = [collect::ninja::collect(dir), collect::cargo::collect(dir)]
        .into_iter()
        .filter(|c| !c.is_empty())
        .collect();
    if collected.is_empty() {
        return None;
    }
    Some(normalize::normalize(collected))
}
