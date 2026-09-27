//! Shared helpers for the compile-check test binaries in this directory.
//!
//! In `tests/common/` rather than `tests/`, so cargo's autotest discovery — which
//! only picks up `tests/*.rs` — does not build it as an extra (empty) test
//! target the way it does `tests/parity_helpers.rs`.

use std::path::{Path, PathBuf};

/// Where a nested `cargo` invocation against a GENERATED throwaway crate puts
/// its lockfile — issue 1392. The call site spells the
/// `resolver.lockfile-path` key itself; this is only the path, derived once.
///
/// THE PROBLEM. These files spawn `Command::new(env!("CARGO"))`, which is the
/// real cargo binary the outer test run exported, not the `scripts/bin/cargo`
/// PATH shim. So the project-wide `--locked` (`NROS_CARGO_FLAGS`, issues
/// 0359/0378) never reaches the nested command and nothing says what it does to
/// a lockfile. `check-nested-cargo-lock-discipline` refuses exactly that, and
/// until issue 1392 it could not see these sites at all: its detector read
/// `env::var("CARGO")` and not the macro spelling every one of them uses.
///
/// WHY NOT `--locked`, WHICH IS THE USUAL ANSWER. The crate being checked was
/// written into a fresh tempdir moments earlier and has no lockfile at all, so
/// `--locked` cannot be satisfied. Measured (cargo 1.98.1, 2026-09-27), on a
/// replica of the crate `generated_heap_message_compiles` writes:
///
/// ```text
/// error: cannot create the lock file …/Cargo.lock because --locked was passed
///        to prevent this
/// ```
///
/// WHAT IS TRUE INSTEAD. The generated manifest carries a `[workspace]` table
/// and the command runs with `current_dir` inside the tempdir, so the tempdir is
/// its own workspace root and the lock is a throwaway. Measured the same day: a
/// `cargo check` of exactly this shape leaves the repo's root `Cargo.lock` byte
/// for byte identical.
///
/// So the honest discipline is the gate's third option, `resolver.lockfile-path`
/// — the same key issue 1307's size probe uses, for the adjacent reason: the
/// file this resolution writes is a build artifact, not the committed promise.
/// It points at a SUBDIRECTORY rather than at the crate root, where the lock
/// would have landed anyway, so the redirect is an enforced statement and not a
/// no-op: measured, after it the generated crate root holds no `Cargo.lock`.
///
/// Cargo requires the path to be named `Cargo.lock`, and honours the key from
/// 1.97 (below that it warns and ignores it, which lands the lock in the crate
/// root — still inside the tempdir, so behaviour is unchanged either way).
pub fn throwaway_lock_path(crate_dir: &Path) -> PathBuf {
    crate_dir.join("nros-throwaway-lock").join("Cargo.lock")
}
