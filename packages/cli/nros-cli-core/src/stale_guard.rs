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

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

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
    let Ok(exe_mtime) = exe.metadata().and_then(|m| m.modified()) else {
        return Ok(());
    };
    let Some(newer) = newest_source_newer_than(&root, exe_mtime) else {
        return Ok(());
    };
    Err(format!(
        "in-tree nros CLI is STALE — source '{newer}' is newer than '{}'.\n\
         A stale CLI silently breaks workspace planning + codegen: its hardcoded\n\
         crate→path table can name locations that no longer exist, and a dropped\n\
         [patch.crates-io] entry resolves from crates.io instead of this checkout\n\
         WITHOUT failing (issues 0363, 0197).\n\
         Rebuild it (not auto-done — compiling at build/test time is forbidden):\n\
         \x20   just setup-cli\n\
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

/// The first tracked CLI source newer than `than`, if any.
///
/// `git ls-files`, not a filesystem walk — the same reasoning as everywhere
/// else in this repo (`scripts/check-no-tracked-file-find.sh`): these are
/// tracked files, so the index already knows them. `third-party/` and
/// `testing_workspaces/` are excluded because they are tracked but are not CLI
/// build inputs, and a parallel session touching them must not read as stale.
fn newest_source_newer_than(root: &Path, than: SystemTime) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "packages/cli"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    for rel in String::from_utf8_lossy(&out.stdout).lines() {
        if !(rel.ends_with(".rs") || rel.ends_with("Cargo.toml") || rel.ends_with("Cargo.lock")) {
            continue;
        }
        if rel.contains("/third-party/") || rel.contains("/testing_workspaces/") {
            continue;
        }
        let p = root.join(rel);
        if let Ok(m) = p.metadata().and_then(|m| m.modified())
            && m > than
        {
            return Some(rel.to_string());
        }
    }
    None
}
