//! Issue 0363 B — the in-tree CLI refuses to run stale.
//!
//! A staleness guard already existed and was good: `scripts/build/cargo.sh`
//! walks `git ls-files packages/cli` for any source newer than the binary and
//! refuses. But it lives in the shell function `nros_cli_bin()`, so it only
//! covers callers that go through `just` — while `activate.sh` puts the raw
//! binary on `PATH`, so a bare `nros sync` never reaches it.
//!
//! That is the whole defect. `nros sync` is the command CLAUDE.md and
//! `nros-patch.toml`'s own header tell you to run to recover, and it was
//! precisely the invocation the protection did not cover. Same shape as issue
//! 0354 (a validator whose callers exclude the case it exists for), with a
//! worse payload: phase-321 moved packages, the stale binary's hardcoded
//! crate→path table still named the old locations, and the emitted
//! `[patch.crates-io]` table DROPPED `nros-zephyr-build` without a word. A
//! dropped patch entry does not fail — the dependency quietly resolves from
//! crates.io instead of the checkout.
//!
//! So the check moves to where it cannot be bypassed by invocation style: the
//! binary checks itself.

use std::path::{Path, PathBuf};

use crate::source_stamp;

/// Stamp of the sources this binary was compiled from, embedded by `build.rs`.
///
/// `"unknown"` when the build happened outside a git checkout (tarball,
/// vendored copy). That is a skip, not a failure: without the tree there is
/// nothing to be stale RELATIVE TO, and guessing would break every packaged
/// install.
const BUILT_STAMP: &str = env!("NROS_CLI_SOURCE_STAMP");

/// Commands that consume the crate→path table or emit generated artifacts.
///
/// Deliberately NOT every command. `nros --version` / `completions` / `doctor`
/// must keep working on a stale binary — `doctor` especially, since diagnosing
/// a broken checkout is exactly when you have one.
fn command_is_guarded(name: &str) -> bool {
    matches!(
        name,
        "sync" | "plan" | "ws" | "codegen" | "codegen-system" | "generate-rust" | "setup"
    )
}

/// Refuse to run when this binary is older than the sources it was built from.
///
/// Only applies to a binary living INSIDE a checkout's `packages/cli/target/`.
/// An installed copy (`~/.nros/bin/nros`) is not "stale relative to" a checkout
/// it does not belong to, and blocking it would break every out-of-tree user.
pub fn refuse_if_stale(command_name: &str) -> Result<(), String> {
    if std::env::var_os("NROS_SKIP_STALE_CHECK").is_some() {
        return Ok(());
    }
    if !command_is_guarded(command_name) {
        return Ok(());
    }
    let Ok(exe) = std::env::current_exe() else {
        return Ok(());
    };
    let Some(root) = checkout_root_of(&exe) else {
        return Ok(());
    };
    if BUILT_STAMP == "unknown" {
        return Ok(());
    }
    // No stamp computable now (git absent / not a checkout) — skip rather than
    // guess. Same reasoning as `BUILT_STAMP == "unknown"`.
    let Some(current) = source_stamp::source_stamp(&root) else {
        return Ok(());
    };
    if current == BUILT_STAMP {
        return Ok(());
    }
    // Name the files actually being edited. The mtime predicate could only
    // report whichever tracked file sorted first, which was frequently not the
    // one the developer had touched.
    let dirty = source_stamp::modified_cli_files(&root);
    let detail = if dirty.is_empty() {
        "  (no uncommitted CLI edits — the checkout moved, e.g. a branch switch)".to_string()
    } else {
        let mut s = String::from("  uncommitted CLI edits:\n");
        for f in dirty.iter().take(3) {
            s.push_str(&format!("    {f}\n"));
        }
        if dirty.len() > 3 {
            s.push_str(&format!("    … and {} more\n", dirty.len() - 3));
        }
        s.trim_end().to_string()
    };
    Err(format!(
        "in-tree nros CLI is STALE — its sources changed since it was built\n\
         (source stamp {BUILT_STAMP} != {current}) for '{}'.\n\
         {detail}\n\
         A stale CLI silently breaks workspace planning + codegen: its hardcoded\n\
         crate→path table can name locations that no longer exist, and a dropped\n\
         [patch.crates-io] entry resolves from crates.io instead of this checkout\n\
         WITHOUT failing (issues 0363, 0197).\n\
         Rebuild it (not auto-done — compiling at build/test time is forbidden):\n\
         \x20   ./scripts/bootstrap.sh      (contributors: just setup-cli)\n\
         Override for a deliberate experiment: NROS_SKIP_STALE_CHECK=1",
        exe.display()
    ))
}

/// `<root>` when `exe` is `<root>/packages/cli/target/**/nros`, else `None`.
fn checkout_root_of(exe: &Path) -> Option<PathBuf> {
    let mut dir = exe.parent()?;
    // walk up looking for the `packages/cli/target` shape
    while let Some(parent) = dir.parent() {
        if dir.file_name().is_some_and(|n| n == "target")
            && parent.file_name().is_some_and(|n| n == "cli")
            && parent
                .parent()?
                .file_name()
                .is_some_and(|n| n == "packages")
        {
            return parent.parent()?.parent().map(Path::to_path_buf);
        }
        dir = parent;
    }
    None
}

/// Report freshness without refusing anything — backs `nros source-stamp`.
///
/// Returns `(built, current)`. Equal means fresh; `None` means the question
/// does not apply here (no stamp, or not a per-checkout binary).
pub fn stamp_pair() -> Option<(String, String)> {
    if BUILT_STAMP == "unknown" {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let root = checkout_root_of(&exe)?;
    let current = source_stamp::source_stamp(&root)?;
    Some((BUILT_STAMP.to_string(), current))
}
