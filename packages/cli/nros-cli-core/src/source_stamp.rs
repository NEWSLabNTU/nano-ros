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

use std::{collections::BTreeSet, path::Path, process::Command};

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
        || rel.ends_with("Cargo.lock")
        // Issue 0604 — the generated closure list DECIDES which dirs the two
        // clauses above are read from, so a stamp blind to it would not move
        // when the watched set changed. Same reasoning as `.jinja`: an input
        // that changes what the build consumes without being a `.rs`.
        || rel == CLI_SOURCE_DIRS_FILE)
        && !rel.contains("/third-party/")
        && !rel.contains("/testing_workspaces/")
}

/// Repo-relative location of the layer-2 resolver submodule (RFC-0060).
const PLAY_LAUNCH_DIR: &str = "packages/cli/third-party/play_launch";

/// The play_launch commit this tree would build against — the one actually
/// CHECKED OUT in the submodule working tree.
///
/// Issue 0561: this is a CLI build input even though it is not a file under any
/// watched directory. `build.rs` bakes it as `NROS_PLAY_LAUNCH_SHA` and the
/// issue-0409 guard compares that value, so a stamp blind to it disagreed with
/// the build in the one case that mattered — moving the pin left the stamp
/// unchanged, `setup-cli` skipped the rebuild while reporting success, and no
/// sanctioned command could clear the resulting mismatch.
///
/// Both callers go through here so they cannot drift: `build.rs` `include!`s
/// this file, so the value baked at build time and the value recomputed at run
/// time come from ONE expression rather than two that happen to agree.
///
/// The SHA, not the submodule's file list — that keeps the "would drag in
/// thousands of files" objection to watching `third-party/` intact while still
/// watching what the build actually consumes.
///
/// `None` when the submodule is not initialised. Gating on the `.git` FILE is
/// issue 0419: an uninitialised submodule is an empty directory that EXISTS,
/// and `git -C <empty dir> rev-parse HEAD` walks UP to the enclosing repo and
/// returns the SUPERPROJECT's HEAD — which would make this component move with
/// every nano-ros commit and re-stale the CLI constantly.
fn play_launch_pin(root: &Path) -> Option<String> {
    let dir = root.join(PLAY_LAUNCH_DIR);
    if !dir.join(".git").exists() {
        return None;
    }
    let sha = git(&dir, &["rev-parse", "HEAD"])?.trim().to_string();
    (!sha.is_empty()).then_some(sha)
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

/// Repo-relative location of the generated closure list (issue 0604).
const CLI_SOURCE_DIRS_FILE: &str = "packages/cli/cli-source-dirs.txt";

/// Directories the CLI is built from: `packages/cli` plus the in-repo crates
/// the `nros` binary is actually compiled from.
///
/// phase-330 W1.a found the need for the second half the hard way. The stamp
/// used to watch `packages/cli` alone, but `nros-cli-core` path-depends on
/// `../../core/nros-orchestration-ir`, which path-depends further. Editing one
/// of those left the CLI genuinely stale while `nros source-stamp` reported
/// FRESH and `setup-cli` skipped the rebuild — so a schema change appeared not
/// to take effect, and the error message kept listing the old fields.
///
/// # Why a generated file and not a walk (issue 0604)
///
/// The fix for that was a textual `path = "…"` walk over manifests, chosen
/// because this file is `include!`d by `build.rs`: it may not pull in a TOML
/// parser, and it cannot shell `cargo metadata` (which takes the package-cache
/// lock cargo already holds during a build). Measured 2026-08-16, that walk was
/// wrong in BOTH directions at once — 23 dirs where cargo resolves 8:
///
/// * **blind to `nros-core` and `nros-rmw`**, which the CLI does compile.
///   `nros-orchestration-ir` spells its dep `nros-rmw = { workspace = true }`,
///   which carries no `path =` at all — the path lives in the ROOT manifest's
///   `[workspace.dependencies]`, a file the leaf-manifest scan never reads. So
///   an edit to either left the stamp FRESH and `setup-cli` skipped the
///   rebuild: exactly the failure this function exists to prevent, reintroduced
///   by its own fix.
/// * **17 dirs over-watched**, all hanging off one edge: `nros-board-common`
///   declares `nros-platform = { optional = true }`, enabled by
///   `deploy-overlay`, which the CLI does not enable. A textual scan cannot see
///   `optional`, so it took every platform port, `nros-node`, `nros-log`,
///   `nros-smoltcp`, `mps2-an385-pac`, `zpico-alloc`, `nros-ghost-types` and
///   three generated msg crates. Editing any of them re-staled the CLI, and a
///   stale CLI re-stales every fixture keyed on its stamp — the cold-leaf
///   cascade issue 0604 was opened to attribute.
///
/// Repairing the walk means hand-rolling workspace-dep inheritance AND
/// optional-dep feature resolution in a file that cannot parse TOML. Cargo
/// computes both exactly, so its answer is recorded instead:
/// `scripts/gen-cli-source-dirs.py` writes the list, this reads it, and
/// `check-cli-source-dirs` fails when the two disagree. The gate is what makes
/// the file safe to trust — without it a stale list is a silent wrong stamp.
///
/// A missing or unreadable file yields `packages/cli` alone, which is the
/// pre-phase-330 behaviour: under-watching, and therefore wrong. It cannot be
/// silent, so [`source_stamp`] refuses to produce a stamp at all in that case.
fn cli_source_dirs(root: &Path) -> Vec<String> {
    let mut dirs = vec!["packages/cli".to_string()];
    let Ok(body) = std::fs::read_to_string(root.join(CLI_SOURCE_DIRS_FILE)) else {
        return dirs;
    };
    let mut seen: BTreeSet<String> = dirs.iter().cloned().collect();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if seen.insert(line.to_string()) {
            dirs.push(line.to_string());
        }
    }
    dirs
}

/// Is the generated closure list present? [`source_stamp`] returns `None`
/// without it rather than stamping over `packages/cli` alone — a smaller
/// closure is a stamp that reports FRESH for a CLI that is not, and "assume
/// fresh" is the one answer a freshness probe must never give.
fn cli_source_dirs_file_present(root: &Path) -> bool {
    root.join(CLI_SOURCE_DIRS_FILE).is_file()
}

/// Every tracked CLI input, repo-relative. Used by `build.rs` for
/// `cargo:rerun-if-changed`.
pub fn cli_input_files(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for dir in cli_source_dirs(root) {
        let Some(listing) = git(root, &["ls-files", "--", &dir]) else {
            continue;
        };
        out.extend(
            listing
                .lines()
                .filter(|l| is_cli_input(l))
                .map(str::to_string),
        );
    }
    out.sort();
    out.dedup();
    out
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
///      new file must not change the stamp of unchanged content.
///
/// Component 2 is what makes a rebase silent: git compares content, so a
/// touched-but-identical file simply is not reported as modified. And blob
/// SHAs are what make a COMMIT silent: identical bytes hash identically
/// whether they sit in the worktree, the index, or HEAD.
pub fn source_stamp(root: &Path) -> Option<String> {
    // Issue 0604 — without the generated closure list the watched set silently
    // shrinks to `packages/cli`, and a smaller closure reports FRESH for a CLI
    // that is not. `None` sends the caller down the same path as "outside a git
    // checkout": skip the check, never guess.
    if !cli_source_dirs_file_present(root) {
        return None;
    }
    let mut h = FNV_OFFSET;
    let modified: std::collections::HashMap<String, String> = {
        let files = modified_cli_files(root);
        hash_objects(root, &files)
            .into_iter()
            .zip(files)
            .map(|(sha, path)| (path, sha))
            .collect()
    };

    // 1. index side (worktree blob SHA substituted for dirty files), over the
    // SAME closure the rest of this file uses — `packages/cli` plus its local
    // path-dep closure. Hardcoding `packages/cli` here is what let an edit to
    // `packages/core/nros-orchestration-ir` leave the stamp unchanged.
    let mut idx = String::new();
    for dir in cli_source_dirs(root) {
        idx.push_str(&git(root, &["ls-files", "-s", "--", &dir])?);
    }
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

    // 4. the play_launch pin (issue 0561). Not a file under any watched
    // directory — and `is_cli_input` excludes `/third-party/` besides — but
    // `build.rs` bakes it into the binary, so by this file's own rule ("any
    // input list here that watches less than what the build consumes is the
    // issue-0196 shape") it belongs in the stamp. Folded in unconditionally,
    // including the uninitialised case, so that `git submodule update --init`
    // moves the stamp: that init changes what the next build bakes.
    h = fnv1a(b"play_launch_pin", h);
    h = fnv1a(
        play_launch_pin(root)
            .unwrap_or_else(|| "unknown".to_string())
            .as_bytes(),
        h,
    );

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
    // Over the SAME closure as the index side. An earlier draft fixed the index
    // half and left this hardcoded to `packages/cli`, so an edit to an
    // out-of-tree path dep changed no component of the stamp and the binary
    // still reported FRESH — the very bug the closure was added to fix,
    // surviving in the other half of the same function.
    let mut out = Vec::new();
    for dir in cli_source_dirs(root) {
        let Some(listing) = git(root, &["diff", "--name-only", "--", &dir]) else {
            continue;
        };
        out.extend(
            listing
                .lines()
                .filter(|l| is_cli_input(l))
                .map(str::to_string),
        );
    }
    out.sort();
    out.dedup();
    out
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

    /// A synthetic checkout needs the generated closure list, or
    /// [`source_stamp`] refuses to stamp at all (issue 0604). Empty is right
    /// here: these fixtures have no crates outside `packages/cli`, so cargo's
    /// resolve for them would name none either.
    fn write_closure_list(root: &Path) {
        std::fs::write(root.join(CLI_SOURCE_DIRS_FILE), "# test fixture\n").unwrap();
    }

    /// Issue 0604 — the closure list decides what is watched, so the two
    /// directions it can be wrong in are the two things worth asserting: a dir
    /// it names is watched, and one it does not name is not.
    ///
    /// The old textual walk got BOTH wrong at once on the real tree (blind to
    /// `workspace = true`, so `nros-rmw` went unwatched; blind to
    /// `optional = true`, so 17 crates the CLI never compiles did). Neither
    /// half had a test, because a walk's closure is only checkable against
    /// cargo — which is now `check-cli-source-dirs`' job, and this is the other
    /// half: that the list is actually obeyed.
    #[test]
    fn the_closure_list_decides_what_is_watched() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        sh(root, "git init -q -b main .");
        std::fs::create_dir_all(root.join("packages/cli/x/src")).unwrap();
        std::fs::write(root.join("packages/cli/x/src/lib.rs"), "fn a() {}\n").unwrap();
        for dir in ["packages/core/listed", "packages/core/unlisted"] {
            std::fs::create_dir_all(root.join(dir).join("src")).unwrap();
            std::fs::write(root.join(dir).join("src/lib.rs"), "fn c() {}\n").unwrap();
        }
        std::fs::write(
            root.join(CLI_SOURCE_DIRS_FILE),
            "# generated\npackages/core/listed\n",
        )
        .unwrap();
        sh(root, "git add -A && git commit -qm init");

        let base = source_stamp(root).expect("stamp in a git checkout");

        std::fs::write(
            root.join("packages/core/unlisted/src/lib.rs"),
            "fn c() {}\nfn d() {}\n",
        )
        .unwrap();
        assert_eq!(
            source_stamp(root).as_deref(),
            Some(base.as_str()),
            "an unlisted dir must not move the stamp — that over-watch is what \
             re-staled the CLI (and through it every fixture) on edits to \
             nros-node and the platform ports"
        );

        std::fs::write(
            root.join("packages/core/listed/src/lib.rs"),
            "fn c() {}\nfn d() {}\n",
        )
        .unwrap();
        assert_ne!(
            source_stamp(root).as_deref(),
            Some(base.as_str()),
            "a listed dir MUST move the stamp — missing this is how an edit to \
             nros-rmw left `setup-cli` reporting success without rebuilding"
        );
    }

    /// Issue 0604 — no list, no stamp. Falling back to `packages/cli` alone
    /// would report FRESH for a CLI whose out-of-cli deps had changed, and
    /// "assume fresh" is the one answer a freshness probe must never give.
    #[test]
    fn a_missing_closure_list_refuses_to_stamp() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        sh(root, "git init -q -b main .");
        std::fs::create_dir_all(root.join("packages/cli/x/src")).unwrap();
        std::fs::write(root.join("packages/cli/x/src/lib.rs"), "fn a() {}\n").unwrap();
        sh(root, "git add -A && git commit -qm init");

        assert!(
            source_stamp(root).is_none(),
            "a checkout with no {CLI_SOURCE_DIRS_FILE} must yield None, not a \
             stamp over a silently smaller closure"
        );
        write_closure_list(root);
        assert!(
            source_stamp(root).is_some(),
            "and it must stamp once the list is there"
        );
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
        write_closure_list(root);
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

    /// Issue 0561 — moving the play_launch pin must move the stamp.
    ///
    /// The pin is a build input (`build.rs` bakes it as `NROS_PLAY_LAUNCH_SHA`)
    /// but it is not a file under any watched directory, so before this it was
    /// invisible to the stamp: `setup-cli` skipped the rebuild while reporting
    /// success, and the 0409 guard then compared a stale baked pin against a
    /// freshly built resolver — a lane no sanctioned command could unstick.
    ///
    /// Built as a real nested repo rather than a fake `.git` file, because the
    /// 0419 behaviour under test (`git -C` walking UP out of an uninitialised
    /// submodule) only reproduces against real git.
    #[test]
    fn moving_the_play_launch_pin_changes_the_stamp() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        sh(root, "git init -q -b main .");
        std::fs::create_dir_all(root.join("packages/cli/x/src")).unwrap();
        write_closure_list(root);
        std::fs::write(root.join("packages/cli/x/src/lib.rs"), "fn a() {}\n").unwrap();
        std::fs::write(
            root.join("packages/cli/x/Cargo.toml"),
            "[package]\nname = \"x\"\nversion = \"0.0.0\"\n",
        )
        .unwrap();
        sh(root, "git add -A && git commit -qm init");

        // Uninitialised: an empty directory that EXISTS. This must NOT pick up
        // the superproject's HEAD (issue 0419).
        let sub = root.join(PLAY_LAUNCH_DIR);
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(
            play_launch_pin(root),
            None,
            "an uninitialised submodule must read as unknown, not as the \
             superproject's HEAD"
        );
        let without = source_stamp(root).expect("stamp without a pin");

        // A real checkout at one commit.
        sh(
            &sub,
            "git init -q -b main . && git commit -q --allow-empty -m one",
        );
        let pin_one = play_launch_pin(root).expect("pin after init");
        let at_one = source_stamp(root).expect("stamp at pin one");
        assert_ne!(
            without, at_one,
            "initialising the submodule changes what the next build bakes, so \
             it must change the stamp"
        );

        // Move the pin. Nothing else in the tree changes.
        sh(&sub, "git commit -q --allow-empty -m two");
        let pin_two = play_launch_pin(root).expect("pin after move");
        assert_ne!(pin_one, pin_two, "the fixture must actually move the pin");
        let at_two = source_stamp(root).expect("stamp at pin two");
        assert_ne!(
            at_one, at_two,
            "moving the play_launch pin must re-stale the CLI (issue 0561)"
        );

        // And it is stable when nothing moves.
        assert_eq!(at_two, source_stamp(root).unwrap(), "stamp must be stable");
    }
}
