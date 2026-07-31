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
/// 2. **Worktree blob SHAs of files git reports as modified.** The index SHA
///    is stale for an unstaged edit — which is the *common* case, someone
///    editing codegen and rebuilding — so those files are re-hashed via
///    `git hash-object`, and their SHA REPLACES the index one **in the same
///    encoding**. Same encoding is load-bearing: hashing dirty content as
///    raw bytes made the stamp of a dirty file differ from the stamp of the
///    SAME bytes once committed, so every `git commit` of CLI sources
///    re-staled a binary built seconds earlier from identical content
///    (observed live 2026-08-01: build → commit → "checkout moved" stale
///    error on the very next verb). Usually zero files, at most a handful.
/// 3. **Untracked-but-present sources**, hashed the same way (`hash-object`
///    + a synthesized index-style meta) for the same reason: committing a
///    new file must not change the stamp of unchanged content.
///
/// Component 2 is what makes a rebase silent: git compares content, so a
/// touched-but-identical file simply is not reported as modified. And blob
/// SHAs are what make a COMMIT silent: identical bytes hash identically
/// whether they sit in the worktree, the index, or HEAD.
pub fn source_stamp(root: &Path) -> Option<String> {
    let mut h = FNV_OFFSET;
    let modified: std::collections::HashMap<String, String> = {
        let files = modified_cli_files(root);
        hash_objects(root, &files)
            .into_iter()
            .zip(files)
            .map(|(sha, path)| (path, sha))
            .collect()
    };

    // 1. index side (worktree blob SHA substituted for dirty files).
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
        let mut fields = meta.split_whitespace();
        let mode = fields.next().unwrap_or("100644");
        let index_sha = fields.next().unwrap_or("");
        let sha = modified.get(path).map(String::as_str).unwrap_or(index_sha);
        h = fnv1a(format!("{mode} {sha} 0").as_bytes(), h);
    }

    // 3. untracked sources — same encoding, mode from the filesystem.
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
        let files: Vec<String> = others
            .lines()
            .filter(|l| is_cli_input(l))
            .map(str::to_string)
            .collect();
        let shas = hash_objects(root, &files);
        for (rel, sha) in files.iter().zip(shas) {
            let mode = file_index_mode(&root.join(rel));
            h = fnv1a(rel.as_bytes(), h);
            h = fnv1a(format!("{mode} {sha} 0").as_bytes(), h);
        }
    }

    Some(format!("{h:016x}"))
}

/// Blob SHAs for `files` (repo-relative), in order — one `git hash-object`
/// batch call. Missing/unreadable files hash to an empty string, which can
/// never equal an index SHA, so a vanished file still perturbs the stamp.
fn hash_objects(root: &Path, files: &[String]) -> Vec<String> {
    if files.is_empty() {
        return Vec::new();
    }
    let mut args: Vec<&str> = vec!["hash-object", "--"];
    args.extend(files.iter().map(String::as_str));
    let out = git(root, &args).unwrap_or_default();
    let mut shas: Vec<String> = out.lines().map(str::to_string).collect();
    shas.resize(files.len(), String::new());
    shas
}

/// The index mode git would record for a filesystem entry (100755 for
/// executables, 100644 otherwise — symlinks/dirs don't reach here through
/// the `is_cli_input` filter).
fn file_index_mode(path: &Path) -> &'static str {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(m) if m.permissions().mode() & 0o111 != 0 => "100755",
        _ => "100644",
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, cmd: &str) {
        let ok = std::process::Command::new("sh")
            .args(["-c", cmd])
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "command failed: {cmd}");
    }

    /// The 2026-08-01 regression this file's encoding rule exists for: a
    /// binary built against DIRTY sources must stay fresh when those exact
    /// bytes are committed — dirty and committed content hash identically
    /// (both as git blob SHAs). A real edit must still change the stamp,
    /// and committing THAT edit must again be a no-op.
    #[test]
    fn commit_of_identical_content_does_not_change_the_stamp() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        sh(root, "git init -q -b main .");
        std::fs::create_dir_all(root.join("packages/cli/x/src")).unwrap();
        std::fs::write(root.join("packages/cli/x/src/lib.rs"), "fn a() {}\n").unwrap();
        std::fs::write(
            root.join("packages/cli/x/Cargo.toml"),
            "[package]\nname=\"x\"\n",
        )
        .unwrap();
        sh(root, "git add -A && git commit -qm init");
        let clean = source_stamp(root).expect("stamp in a git checkout");

        // Unstaged edit → stamp changes.
        std::fs::write(
            root.join("packages/cli/x/src/lib.rs"),
            "fn a() {}\nfn b() {}\n",
        )
        .unwrap();
        let dirty = source_stamp(root).expect("stamp with dirty file");
        assert_ne!(clean, dirty, "a real edit must change the stamp");

        // Committing the SAME bytes → stamp unchanged (the regression).
        sh(root, "git add -A && git commit -qm edit");
        let committed = source_stamp(root).expect("stamp after commit");
        assert_eq!(
            dirty, committed,
            "identical bytes must stamp identically whether dirty or committed \
             — otherwise every commit re-stales a binary built seconds earlier"
        );

        // An untracked new source also stamps stably across its commit.
        std::fs::write(root.join("packages/cli/x/src/new.rs"), "fn c() {}\n").unwrap();
        let with_untracked = source_stamp(root).unwrap();
        assert_ne!(
            committed, with_untracked,
            "a new source must change the stamp"
        );
        sh(root, "git add -A && git commit -qm new");
        assert_eq!(
            with_untracked,
            source_stamp(root).unwrap(),
            "committing an untracked source must not change the stamp"
        );
    }
}
