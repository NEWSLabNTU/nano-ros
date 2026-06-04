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
//! - [`check_workspace`] — walks up from `start` looking for `Cargo.lock`,
//!   parses it, finds the resolved `nros-core` version, compares to the CLI
//!   binary's embedded version ([`CLI_VERSION`]), and either continues or
//!   exits with an actionable error message naming both versions plus the
//!   fix command.
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
        let Some(lock) = find_cargo_lock(start) else {
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
