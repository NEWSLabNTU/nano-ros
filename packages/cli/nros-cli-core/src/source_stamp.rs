// Content stamp over the CLI's own sources — the freshness predicate.
//
// NOTE: plain `//` comments, not `//!`. This file is `include!`d by `build.rs`,
// where the expansion lands after other items and an inner doc comment is a
// hard error (E0753). Module-level rustdoc lives on the `pub mod source_stamp;`
// declaration in `lib.rs` instead.
//
// `include!`d by `build.rs` (compute at build time, embed the result) and
// compiled into the crate (recompute at run time, compare). ONE implementation,
// two callers, which is the point: before this there were three spellings of
// "is the CLI stale", two of them real implementations.
//
// WHY CONTENT AND NOT MTIME
//
// The previous predicate compared mtimes: any CLI source newer than the binary
// meant stale. That is wrong in a way that shows up constantly — `git rebase`,
// `git stash pop`, and `git checkout` all rewrite tracked files with IDENTICAL
// content, refreshing mtimes and making a perfectly good binary read stale.
// Observed live: a rebase that pulled three unrelated commits touched
// `cmd/codegen.rs` without changing a byte of it, and the mtime guard fired.
//
// CLAUDE.md already names this the "fixture mtime treadmill" for prebuilt
// fixtures. Here the fix is available, because the inputs are all tracked
// files: hash what the sources ARE instead of when they were written. A rebase
// becomes silent; an actual codegen edit is still caught.
//
// WHY THIS IS NOT A SECURITY BOUNDARY
//
// FNV-1a rather than SHA-2, hand-rolled rather than a `sha2` build-dependency.
// This detects accidental drift between a binary and the tree it came from. It
// defends against nobody, so collision resistance buys nothing, and a
// build-dependency would have to resolve in every `--locked` build of this
// sub-workspace for no gain.

use std::{path::Path, process::Command};

/// FNV-1a, 64-bit. Stable by construction: the constants are in this file, so
/// `build.rs` and the runtime cannot disagree the way two std-hasher versions
/// could.
fn fnv1a(bytes: &[u8], mut h: u64) -> u64 {
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// Is this repo-relative path a CLI build input?
///
/// `third-party/` and `testing_workspaces/` are tracked but are not inputs —
/// they are vendored submodules and CLI-test fixtures. A parallel session
/// touching them must not read as CLI staleness.
///
/// `.jinja` belongs here for a non-obvious reason (phase-318 W1): askama
/// compiles the templates INTO the binary, so a template-only edit changes the
/// bytes the CLI emits while touching no `.rs`. The `setup-cli` filter learned
/// this the hard way — the codegen fingerprint refused to move after a template
/// edit because the freshness scan could not see it. Any input list here that
/// watches less than what the build consumes is the issue-0196 shape.
fn is_cli_input(rel: &str) -> bool {
    (rel.ends_with(".rs")
        || rel.ends_with(".jinja")
        || rel.ends_with("Cargo.toml")
        || rel.ends_with("Cargo.lock"))
        && !rel.contains("/third-party/")
        && !rel.contains("/testing_workspaces/")
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every tracked CLI input, repo-relative. Used by `build.rs` for
/// `cargo:rerun-if-changed`.
pub fn cli_input_files(root: &Path) -> Vec<String> {
    let Some(out) = git(root, &["ls-files", "--", "packages/cli"]) else {
        return Vec::new();
    };
    out.lines()
        .filter(|l| is_cli_input(l))
        .map(str::to_string)
        .collect()
}

/// A stamp of the CLI sources as they exist right now, or `None` outside a git
/// checkout (a tarball build, a vendored copy) — in which case the caller must
/// skip the check rather than guess.
///
/// Three components, because "what the sources are" is not one question:
///
/// 1. **Index blob SHAs** (`ls-files -s`). Cheap: git already computed these,
///    so nothing is re-hashed. Covers committed and staged content.
/// 2. **Worktree content of files git reports as modified.** The index SHA is
///    stale for an unstaged edit — which is the *common* case, someone editing
///    codegen and rebuilding — so those files are hashed from disk. Usually
///    zero files, at most a handful.
/// 3. **Untracked-but-present sources.** A new `.rs` reachable via `mod` is a
///    real input the index has never seen.
///
/// Component 2 is what makes a rebase silent: git compares content, so a
/// touched-but-identical file simply is not reported as modified.
pub fn source_stamp(root: &Path) -> Option<String> {
    let mut h = FNV_OFFSET;

    // 1. index side
    let idx = git(root, &["ls-files", "-s", "--", "packages/cli"])?;
    for line in idx.lines() {
        // "<mode> <sha> <stage>\t<path>" — skip anything that does not parse
        // rather than aborting the whole stamp on one odd entry.
        let Some((meta, path)) = line.split_once('\t') else {
            continue;
        };
        if !is_cli_input(path) {
            continue;
        }
        h = fnv1a(path.as_bytes(), h);
        h = fnv1a(meta.as_bytes(), h);
    }

    // 2. worktree side — only files git says actually differ
    for rel in modified_cli_files(root) {
        h = fnv1a(rel.as_bytes(), h);
        if let Ok(content) = std::fs::read(root.join(&rel)) {
            h = fnv1a(&content, h);
        }
    }

    // 3. untracked sources
    if let Some(others) = git(
        root,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--",
            "packages/cli",
        ],
    ) {
        for rel in others.lines().filter(|l| is_cli_input(l)) {
            h = fnv1a(rel.as_bytes(), h);
            if let Ok(content) = std::fs::read(root.join(rel)) {
                h = fnv1a(&content, h);
            }
        }
    }

    Some(format!("{h:016x}"))
}

/// CLI inputs whose worktree content differs from the index.
///
/// Also used for the error message: naming the files someone is editing is far
/// more useful than naming whichever file happened to sort first, which is all
/// the mtime predicate could report.
pub fn modified_cli_files(root: &Path) -> Vec<String> {
    let Some(out) = git(root, &["diff", "--name-only", "--", "packages/cli"]) else {
        return Vec::new();
    };
    out.lines()
        .filter(|l| is_cli_input(l))
        .map(str::to_string)
        .collect()
}
