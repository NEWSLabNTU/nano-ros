//! Phase 218.E — ABI version guard between the prebuilt `nros` CLI and the
//! `nros-core` runtime crate the consumer's `Cargo.lock` resolves.
//!
//! The CLI emits Rust / C / C++ code that targets `nros-core`, `nros-c`, and
//! `nros-cpp` runtime ABIs. A version mismatch between the CLI binary and the
//! runtime crates the consumer links surfaces as link errors, struct-layout
//! mismatches, or — worst — silent runtime UB. The guard catches this BEFORE
//! codegen runs.
//!
//! ## Surface
//!
//! - [`check_workspace`] — resolves the authoritative `Cargo.lock`, parses
//!   it, finds the resolved `nros-core` version, compares to the CLI
//!   binary's embedded version ([`CLI_VERSION`]), and either continues or
//!   exits with an actionable error message naming both versions plus the
//!   fix command. For consumers inside the nano-ros monorepo the lock is the
//!   monorepo root lock (in-tree examples link the patched in-tree
//!   `nros-core`, so a standalone crate's own — possibly stale — lock is not
//!   authoritative); external consumers use the nearest `Cargo.lock`.
//! - [`check_workspaces`] — the multi-workspace variant for verbs that resolve
//!   inputs from more than one workspace (e.g. `nros codegen-system`).
//!
//! ## Opt-out
//!
//! `NROS_SKIP_VERSION_CHECK=1` in the environment bypasses the check, with a
//! `warning:` line on stderr so the bypass is visible in CI logs.
//!
//! ## Match rule
//!
//! Strict equality on the full SemVer string today. The comparison fn
//! ([`versions_match`]) is the single point to relax later (e.g. to
//! "MAJOR.MINOR equal").
//!
//! ## Missing `Cargo.lock`
//!
//! If the consumer has not run `cargo generate-lockfile` yet, the guard
//! warns-and-continues — the codegen step the verb is about to run will
//! probably create the lock itself.

use std::{
    env,
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
};

use eyre::{Context, Result, bail};

/// The CLI binary's embedded version — baked at compile time from the
/// `nros-cli-core` crate's `CARGO_PKG_VERSION`.
///
/// Today this is *the CLI's own version*; the `nros-cli` workspace bumps in
/// lockstep with the runtime ABI it targets. A future refactor may split the
/// "CLI version" from the "runtime ABI version" (e.g. via a build script that
/// resolves the in-tree `nros-core` version); the call sites here would not
/// change.
pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Env-var name that bypasses the guard (any non-empty value opts out).
pub const SKIP_ENV: &str = "NROS_SKIP_VERSION_CHECK";

/// The codegen verb that triggered the check — flows into the error
/// message so the user knows which command tripped the guard.
#[derive(Debug, Clone, Copy)]
pub enum Verb {
    Codegen,
    GenerateRust,
    GenerateC,
    GenerateCpp,
    CodegenSystem,
}

impl Verb {
    fn as_str(self) -> &'static str {
        match self {
            Verb::Codegen => "nros codegen",
            Verb::GenerateRust => "nros generate-rust",
            Verb::GenerateC => "nros generate c",
            Verb::GenerateCpp => "nros generate cpp",
            Verb::CodegenSystem => "nros codegen-system",
        }
    }
}

/// Walk up from `start` looking for a `Cargo.lock`. Returns `None` if none
/// is found above the filesystem root.
pub fn find_cargo_lock(start: &Path) -> Option<PathBuf> {
    let mut cur: Option<&Path> = if start.is_file() {
        start.parent()
    } else {
        Some(start)
    };
    while let Some(dir) = cur {
        let candidate = dir.join("Cargo.lock");
        if candidate.is_file() {
            return Some(candidate);
        }
        cur = dir.parent();
    }
    None
}

/// Walk up from `start` to find the nano-ros monorepo root — the directory
/// containing `packages/core/nros-core/Cargo.toml`. Returns `None` when
/// `start` is not inside the monorepo (a genuine external consumer), in
/// which case the nearest-`Cargo.lock` rule applies.
pub fn find_monorepo_root(start: &Path) -> Option<PathBuf> {
    const MARKER: &str = "packages/core/nros-core/Cargo.toml";
    let mut cur: Option<&Path> = if start.is_file() {
        start.parent()
    } else {
        Some(start)
    };
    while let Some(dir) = cur {
        if dir.join(MARKER).is_file() {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

/// The monorepo root's `Cargo.lock`, if `start` is inside the monorepo and
/// that lock exists.
///
/// In-tree examples / test crates are standalone projects that link the
/// in-tree `nros-core` via `[patch.crates-io]`, so a standalone crate's own
/// `Cargo.lock` can be stale (e.g. pinned to a pre-bump `0.1.0` long after
/// the workspace moved to `0.5.0`) while the crate still builds against the
/// current in-tree ABI. The monorepo root lock is the authoritative source
/// for those consumers; external consumers fall back to the nearest lock.
fn monorepo_root_lock(start: &Path) -> Option<PathBuf> {
    let lock = find_monorepo_root(start)?.join("Cargo.lock");
    lock.is_file().then_some(lock)
}

/// Parse a `Cargo.lock` (TOML) and pull out the resolved version of the named
/// package, if present.
///
/// Returns `None` if the package is not in the lock — e.g. the consumer has
/// not yet declared a dependency on `nros-core`. That is not an error; the
/// caller treats it as "nothing to check".
pub fn nros_core_version_in_lock(lock_path: &Path) -> Result<Option<String>> {
    let body = std::fs::read_to_string(lock_path)
        .wrap_err_with(|| format!("read Cargo.lock at {}", lock_path.display()))?;
    let parsed: toml::Value = toml::from_str(&body)
        .wrap_err_with(|| format!("parse Cargo.lock at {}", lock_path.display()))?;
    let Some(packages) = parsed.get("package").and_then(|v| v.as_array()) else {
        return Ok(None);
    };
    for pkg in packages {
        let Some(name) = pkg.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        if name != "nros-core" {
            continue;
        }
        if let Some(ver) = pkg.get("version").and_then(|v| v.as_str()) {
            return Ok(Some(ver.to_string()));
        }
    }
    Ok(None)
}

/// Comparison rule. Strict equality today. The single point to relax later
/// (e.g. compare only `MAJOR.MINOR`).
pub fn versions_match(cli: &str, consumer: &str) -> bool {
    cli == consumer
}

/// Run the guard against a single workspace anchor.
///
/// `start` is either a file (the `package.xml` / `args-file` / `system.toml`
/// the verb received) or a directory (the workspace root); the guard walks up
/// from there to find a `Cargo.lock`.
///
/// Returns `Ok(())` on match, opt-out, or no-lock-found.
/// Returns `Err(...)` on mismatch with an actionable message.
pub fn check_workspace(start: &Path, verb: Verb) -> Result<()> {
    check_workspaces(std::slice::from_ref(&start), verb)
}

/// Multi-workspace variant — checks each anchor in turn and fails on the
/// first mismatch. Used by verbs that resolve inputs from more than one
/// workspace (e.g. `nros codegen-system` reading a bringup pkg from
/// workspace A + member pkgs from workspace B).
pub fn check_workspaces(starts: &[&Path], verb: Verb) -> Result<()> {
    if env::var(SKIP_ENV).map(|v| !v.is_empty()).unwrap_or(false) {
        warn_bypass(verb);
        return Ok(());
    }

    let mut checked_locks: Vec<PathBuf> = Vec::new();
    for start in starts {
        // Prefer the nano-ros monorepo root lock for in-tree consumers: a
        // standalone example/test under the nano-ros tree links the in-tree
        // `nros-core` via `[patch.crates-io]`, so its own (possibly stale)
        // `Cargo.lock` does not reflect the ABI it actually builds against.
        // External consumers (no monorepo marker above `start`) keep the
        // nearest-lock rule.
        let lock = monorepo_root_lock(start).or_else(|| find_cargo_lock(start));
        let Some(lock) = lock else {
            // No lock — codegen verb will likely create one. Warn so the
            // skip is visible.
            warn_no_lock(start, verb);
            continue;
        };
        if checked_locks.iter().any(|prev| prev == &lock) {
            // Same workspace as a previous anchor.
            continue;
        }
        let Some(consumer_version) = nros_core_version_in_lock(&lock)? else {
            // `nros-core` not in lock — consumer hasn't declared a dep on it
            // yet. Nothing to check.
            checked_locks.push(lock);
            continue;
        };
        if !versions_match(CLI_VERSION, &consumer_version) {
            bail!(
                "{}",
                mismatch_message(verb, &lock, &consumer_version, CLI_VERSION)
            );
        }
        checked_locks.push(lock);
    }
    Ok(())
}

fn warn_bypass(verb: Verb) {
    let _ = writeln!(
        std::io::stderr(),
        "warning: {} ABI version guard bypassed via {SKIP_ENV}=1 \
         (CLI nros-core = {CLI_VERSION})",
        verb.as_str(),
    );
}

fn warn_no_lock(start: &Path, verb: Verb) {
    // Quiet unless verbose-ish: still noteworthy in CI logs. Only emit
    // when stderr is a TTY OR when the env requests verbose tracing —
    // otherwise pure-codegen verbs (which legitimately run before a lock
    // exists) get spammy. Cheapest heuristic that still surfaces in CI: a
    // single line, gated only on the env var.
    if env::var("NROS_TRACE_ABI_GUARD")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
        || std::io::stderr().is_terminal()
    {
        let _ = writeln!(
            std::io::stderr(),
            "note: {} found no Cargo.lock at or above {}; skipping ABI version guard",
            verb.as_str(),
            start.display(),
        );
    }
}

fn mismatch_message(verb: Verb, lock: &Path, consumer: &str, cli: &str) -> String {
    format!(
        "{verb} aborted: ABI version mismatch between the `nros` CLI binary \
         and the runtime `nros-core` your workspace resolves.\n  \
         CLI binary nros-core version: {cli}\n  \
         Workspace Cargo.lock at:      {lock}\n  \
         Workspace nros-core version:  {consumer}\n\n\
         The CLI emits Rust / C / C++ that targets a specific `nros-core` ABI; \
         a mismatch can manifest as link errors, struct-layout mismatches, or \
         silent runtime UB. Resolve by rebuilding the CLI against this workspace's \
         pinned runtime:\n  \
         cargo build --release --manifest-path /path/to/nano-ros/packages/cli/Cargo.toml --bin nros\n\
         (or `just setup-cli` if the target workspace IS nano-ros itself).\n\n\
         To bypass this guard for an intentional cross-version workflow, set \
         {SKIP_ENV}=1.",
        verb = verb.as_str(),
        cli = cli,
        lock = lock.display(),
        consumer = consumer,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_match_is_strict_equality() {
        assert!(versions_match("0.3.7", "0.3.7"));
        assert!(!versions_match("0.3.7", "0.3.8"));
        assert!(!versions_match("0.3.7", "0.0.999"));
    }

    #[test]
    fn find_cargo_lock_walks_up() {
        let tmp = tempdir_path("abi_guard_find_lock");
        std::fs::create_dir_all(tmp.join("a/b/c")).unwrap();
        std::fs::write(tmp.join("Cargo.lock"), "# stub\n").unwrap();
        let found = find_cargo_lock(&tmp.join("a/b/c")).expect("lock found");
        assert_eq!(found, tmp.join("Cargo.lock"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn extract_nros_core_version_from_lock() {
        let tmp = tempdir_path("abi_guard_extract");
        std::fs::create_dir_all(&tmp).unwrap();
        let lock = tmp.join("Cargo.lock");
        std::fs::write(
            &lock,
            r#"
[[package]]
name = "foo"
version = "1.2.3"

[[package]]
name = "nros-core"
version = "0.0.999"

[[package]]
name = "bar"
version = "4.5.6"
"#,
        )
        .unwrap();
        let v = nros_core_version_in_lock(&lock).unwrap();
        assert_eq!(v.as_deref(), Some("0.0.999"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn monorepo_root_lock_wins_over_nested_stale_lock() {
        let tmp = tempdir_path("abi_guard_monorepo");
        // Monorepo root: the nros-core marker + a fresh root lock (0.5.0).
        std::fs::create_dir_all(tmp.join("packages/core/nros-core")).unwrap();
        std::fs::write(tmp.join("packages/core/nros-core/Cargo.toml"), "# stub\n").unwrap();
        std::fs::write(
            tmp.join("Cargo.lock"),
            "[[package]]\nname = \"nros-core\"\nversion = \"0.5.0\"\n",
        )
        .unwrap();
        // Nested standalone example with a STALE own-lock (0.1.0).
        let ex = tmp.join("examples/x/rust/talker");
        std::fs::create_dir_all(&ex).unwrap();
        std::fs::write(
            ex.join("Cargo.lock"),
            "[[package]]\nname = \"nros-core\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        // The monorepo root is discovered from the nested dir...
        let root = find_monorepo_root(&ex).expect("monorepo root found");
        assert_eq!(root, tmp);
        // ...and its lock (0.5.0) is the authoritative source, not the
        // nested stale 0.1.0 lock that find_cargo_lock would return.
        let nearest = find_cargo_lock(&ex).unwrap();
        assert_eq!(
            nros_core_version_in_lock(&nearest).unwrap().as_deref(),
            Some("0.1.0")
        );
        assert_eq!(
            nros_core_version_in_lock(&root.join("Cargo.lock"))
                .unwrap()
                .as_deref(),
            Some("0.5.0")
        );

        // A dir outside any monorepo marker has no monorepo root.
        let outside = tempdir_path("abi_guard_outside");
        std::fs::create_dir_all(&outside).unwrap();
        assert!(find_monorepo_root(&outside).is_none());

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn extract_returns_none_when_absent() {
        let tmp = tempdir_path("abi_guard_absent");
        std::fs::create_dir_all(&tmp).unwrap();
        let lock = tmp.join("Cargo.lock");
        std::fs::write(
            &lock,
            r#"
[[package]]
name = "foo"
version = "1.2.3"
"#,
        )
        .unwrap();
        let v = nros_core_version_in_lock(&lock).unwrap();
        assert_eq!(v, None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn tempdir_path(tag: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("phase-218-e-{tag}-{}-{stamp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }
}
