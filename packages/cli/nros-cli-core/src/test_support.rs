//! Issue 0455 — the ONE way a test in this crate names a scratch directory.
//!
//! Before this module there were nine hand-written spellings of
//! `std::env::temp_dir().join(...)`, and the differences between them were the
//! bug. Two shared a base name and raced each other. Four keyed only on a
//! `{n}` counter, so two processes agreed on the path. Three keyed on a
//! nanosecond `{stamp}`, which two runs in the same tick still collide on. One
//! had no discriminator at all.
//!
//! The observed failure was `ETXTBSY`: a test writes an executable stub and
//! exec's it while a concurrent run truncates the same path. The quieter mode
//! hits every site — these helpers begin with `remove_dir_all`, so one run
//! deletes another's scratch mid-test. Because the panic names whichever verb
//! was running, it reads as a codegen regression rather than a harness defect,
//! which is what made it expensive to place.
//!
//! Uniqueness here does not depend on the clock. A path is
//! `<base>/nros-cli-core-tests-<pid>/<tag>-<seq>`: the pid separates processes,
//! and the process-wide `SEQ` separates calls within one. Two runs in the same
//! nanosecond are therefore fine, which is not true of a `{stamp}`.
//!
//! `CARGO_TARGET_TMPDIR` is honoured as the base when cargo sets it, but the
//! pid segment is appended EITHER WAY. The three call sites this replaces put
//! the pid only in the fallback — and cargo hands the same
//! `CARGO_TARGET_TMPDIR` to every run of a given test binary, so two concurrent
//! runs of one integration test would still have shared a path. That hole is
//! latent today only because `check-cli-tests` runs `--lib`, where the variable
//! is absent.
//!
//! Sweep for regressions of the class:
//!
//! ```text
//! git grep -n 'temp_dir()' -- packages/cli | grep -v 'process::id()'
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Per-call discriminator, so one process's tests never collide with each
/// other even when they pass the same tag.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// This PROCESS's scratch root. Everything a run creates lives under here, so
/// a `remove_dir_all` can only ever clear our own tree.
fn scratch_root() -> PathBuf {
    std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("nros-cli-core-tests-{}", std::process::id()))
}

/// A unique scratch path that does NOT exist. Nothing is created — for the
/// callers that want to hand a missing directory to the code under test.
pub(crate) fn scratch_path(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    scratch_root().join(format!("{tag}-{n}"))
}

/// A unique, EMPTY scratch directory, created and ready to write into.
pub(crate) fn scratch_dir(tag: &str) -> PathBuf {
    let dir = scratch_path(tag);
    // Fresh by construction (SEQ never repeats), so this only matters if a
    // previous run of THIS pid left something behind — a pid the OS reused.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|e| panic!("create scratch dir {}: {e}", dir.display()));
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the issue turns on: same tag, different path.
    #[test]
    fn same_tag_twice_yields_distinct_dirs() {
        let a = scratch_dir("dup");
        let b = scratch_dir("dup");
        assert_ne!(a, b, "two calls with one tag must not share a directory");
        assert!(a.is_dir() && b.is_dir());
    }

    /// The pid segment is what separates concurrent processes, and it must be
    /// present whether or not cargo set `CARGO_TARGET_TMPDIR`.
    #[test]
    fn path_is_scoped_to_this_process() {
        let pid = std::process::id().to_string();
        let p = scratch_path("scoped");
        assert!(
            p.components()
                .any(|c| c.as_os_str().to_string_lossy().contains(&pid)),
            "scratch path {} carries no pid segment",
            p.display()
        );
    }

    /// `scratch_path` promises a path that is not there yet.
    #[test]
    fn scratch_path_does_not_create() {
        assert!(!scratch_path("absent").exists());
    }
}
