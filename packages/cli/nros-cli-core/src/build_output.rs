//! Is this directory build output? — one predicate, every dirwalk.
//!
//! Several places walk a user's workspace looking for sources and must not
//! descend into build output: the metadata source hash, `nros check-workspace`,
//! the build profiler. Each grew its own spelling, and each spelling was an
//! EXACT match on `target` / `build`.
//!
//! That is not what this repo's trees look like. The convention is a per-RMW or
//! per-board suffix — `target-zenoh`, `target-xrce`, `target-cyclonedds`,
//! `build-zenoh`, `build-workspace-fixtures-nuttx` — and none of those equal
//! `target` or `build`. So every exact-match walker descended into build output
//! and hashed (or diagnosed, or warned about) whatever cargo happened to have
//! written there.
//!
//! It surfaced in phase-338 W3 step 4: the source hash recursed into
//! `service-server/target-xrce/`, read cargo's INCREMENTAL artifacts, and died
//! on a scratch file cargo deleted from under it mid-walk —
//!
//! ```text
//! Error: read …/target-xrce/nros-fast-release/incremental/…-working/dep-graph.part.bin
//! Caused by: No such file or directory (os error 2)
//! ```
//!
//! — failing a 55-minute fixture build from a race that was pure collateral:
//! nothing under `target-*` was ever a legitimate hash input.
//!
//! Prefix-match, in one place, so the next suffix someone invents is covered
//! without a fourth copy of the list.

/// Directory names that are build output outright.
const EXACT: &[&str] = &["target", "build", "generated", "metadata", "node_modules"];

/// Prefixes for the suffixed variants (`target-xrce`, `build-zenoh`, …).
const PREFIXES: &[&str] = &["target-", "build-"];

/// True when a directory of this name holds build output and a source walk
/// should not descend into it.
///
/// Takes the FILE NAME, not a path — callers already have it from `read_dir`.
pub fn is_build_output_dir(name: &str) -> bool {
    EXACT.contains(&name) || PREFIXES.iter().any(|p| name.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::is_build_output_dir;

    #[test]
    fn plain_build_output_names_are_skipped() {
        for name in ["target", "build", "generated", "metadata", "node_modules"] {
            assert!(is_build_output_dir(name), "{name} should be skipped");
        }
    }

    #[test]
    fn suffixed_build_output_names_are_skipped() {
        // The class that every exact-match walker missed.
        for name in [
            "target-zenoh",
            "target-xrce",
            "target-cyclonedds",
            "build-zenoh",
            "build-workspace-fixtures-nuttx",
        ] {
            assert!(is_build_output_dir(name), "{name} should be skipped");
        }
    }

    #[test]
    fn source_directories_are_not_skipped() {
        // Guard the prefix rule against over-reach: a package legitimately
        // named `targeting` or `builder` is source, not output.
        for name in ["src", "targeting", "builder", "targets", "buildings"] {
            assert!(!is_build_output_dir(name), "{name} should NOT be skipped");
        }
    }
}
