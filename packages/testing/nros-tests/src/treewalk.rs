//! One answer to "is this directory build output?", for every tree walker.
//!
//! Five walkers in this crate each had their own spelling of the rule, and they
//! did not agree:
//!
//! | walker | skipped |
//! | --- | --- |
//! | `example_shape.rs` | `build-*`, `target-*`, `target`, `build`, … (correct) |
//! | `cpp_api_drift.rs` | `build-*`, `target*` (correct by accident) |
//! | `examples_canonical_shape.rs` | `build-*`, `target` — **missed `target-*`** |
//! | `zephyr.rs` (×2) | `target`, `build`, `.git` — **missed both prefixes** |
//! | `diagnostic_verbatim.rs` | `target`, `build` — **missed both prefixes** |
//!
//! The prefixes are not exotic: the repo builds each RMW and feature variant into
//! its own dir precisely so they do not overwrite one another — `build-zenoh/`,
//! `target-xrce/`, `target-safety/`, `target-zero-copy/` — and every per-example
//! `.gitignore` lists them. A walker that knows `target` but not `target-xrce`
//! knows the convention only where it was written down twice.
//!
//! What the divergence cost:
//!
//! - `examples_canonical_shape` walked 48 cargo target dirs and blew past
//!   nextest's 60s timeout — but only on a machine that had run the native RMW
//!   sweep, so it stayed green on fresh checkouts and failed for whoever was
//!   doing the work.
//! - `zephyr::collect_source_files` feeds the fixture SIGNATURE. Hashing build
//!   output into a source signature means the signature changes whenever a build
//!   runs, which is the fixture-staleness treadmill with an extra step.
//! - `zephyr`'s mtime staleness walker descended into build dirs, whose mtimes are
//!   newer than any cutoff by construction — it could only ever answer "stale".
//! - `diagnostic_verbatim::copy_tree` copied whatever build output a local
//!   `cargo check` had left in the fixture.
//!
//! So: one predicate, used by all of them.

/// True when a directory entry named `name` is build output rather than source.
///
/// Matches the exact names AND the `build-<variant>` / `target-<variant>` forms
/// the per-variant build convention produces. Callers pass the FILE NAME, not a
/// path.
pub fn is_build_output_dir(name: &str) -> bool {
    if name.starts_with("build-") || name.starts_with("target-") {
        return true;
    }
    matches!(
        name,
        "target" | "build" | "node_modules" | "cmake-build-debug" | "cmake-build-release"
    )
}

/// `is_build_output_dir`, plus the VCS/tooling dirs and generated output no
/// source walker wants to descend into.
///
/// Separate from [`is_build_output_dir`] because not every caller agrees about
/// `generated/`: it is derived, but a shape checker may legitimately want to see
/// it. Callers that skip it say so by choosing this predicate.
pub fn is_skipped_dir(name: &str) -> bool {
    is_build_output_dir(name) || matches!(name, ".git" | ".cargo" | "generated")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_variant_build_dirs_are_build_output() {
        // The exact names the tree actually uses — the ones the divergent
        // spellings missed.
        for name in [
            "target",
            "target-xrce",
            "target-zenoh",
            "target-cyclonedds",
            "target-safety",
            "target-zero-copy",
            "target-tls",
            "build",
            "build-zenoh",
            "build-cyclonedds",
        ] {
            assert!(
                is_build_output_dir(name),
                "{name} must be treated as build output"
            );
        }
    }

    #[test]
    fn source_dirs_are_not_build_output() {
        // `targets` and `builder` share a prefix with the rule but are not it;
        // a `starts_with("target")` spelling would wrongly skip them.
        for name in ["src", "msg", "launch", "config", "targets", "builder"] {
            assert!(!is_build_output_dir(name), "{name} must not be skipped");
        }
    }

    #[test]
    fn skipped_dir_adds_vcs_and_generated_but_keeps_build_output() {
        assert!(is_skipped_dir(".git"));
        assert!(is_skipped_dir("generated"));
        assert!(is_skipped_dir("target-xrce"));
        assert!(!is_skipped_dir("src"));
        // `generated/` is skipped only by the broader predicate.
        assert!(!is_build_output_dir("generated"));
    }
}
