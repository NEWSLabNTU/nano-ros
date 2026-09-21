//! Embeds a content stamp of the CLI's sources into the binary.
//!
//! This is the "let the build system do it" half of issue 0363's simplification.
//! The freshness question — "was this binary built from these sources?" — is
//! answered by comparing a stamp taken HERE against one recomputed at run time,
//! rather than by a shell script comparing mtimes from outside.
//!
//! Cargo cannot answer the question itself, and it is worth being precise about
//! why, because for most consumers it CAN: Rust Entry packages call codegen as a
//! linked library (`nros-build` depends on `nros-cli-core` for exactly this
//! reason), so cargo fingerprints it normally and no guard is involved. Two
//! consumers are outside that graph:
//!
//!   * CMake / C / C++ — `find_program(nros)`, no cargo in the picture at all.
//!   * `nros sync` — it WRITES `.cargo/config.toml` and `nros-patch.toml`, i.e.
//!     it generates cargo's own input. A tool that produces the configuration
//!     cargo reads cannot be fingerprinted by cargo. That is bootstrap
//!     ordering, not a missing cargo feature.
//!
//! So the stamp exists for those two, and nothing else.

// `source_stamp.rs` is shared with the crate proper; build.rs uses only part of
// its surface, and an unused-function warning here would be noise, not signal.
#![allow(dead_code)]

use std::path::PathBuf;

include!("src/source_stamp.rs");
include!("../build-support/submodule_watch.rs");

fn main() {
    // <root>/packages/cli/nros-cli-core -> <root>
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(PathBuf::from);

    let Some(root) = root else {
        println!("cargo:rustc-env=NROS_CLI_SOURCE_STAMP=unknown");
        return;
    };

    // Rerun when any CLI source changes. Emitting an explicit list REPLACES
    // cargo's default (rerun on any change in this package), so the list must
    // cover the whole sub-workspace — a change in `rosidl-codegen` must restamp
    // this crate even though it lives elsewhere.
    for rel in cli_input_files(&root) {
        println!("cargo:rerun-if-changed={}", root.join(&rel).display());
    }
    // …and when the index moves (commit, rebase, branch switch), since the
    // stamp reads index blob SHAs.
    //
    // ASK git where that file is; never join `.git/<x>` onto the checkout
    // (issues 1336 + 1306). This line read `root.join(".git/index")`, which is
    // right only for a main checkout: in a linked worktree `.git` is a FILE and
    // the index is under `<common>/.git/worktrees/<name>/`, so the path did not
    // exist, the `exists()` guard failed OPEN, and nothing watched the index at
    // all. Measured cost in a worktree: commit a new CLI source, and
    // `just setup-cli` loops forever reporting `built:` while `check cli-fresh`
    // stays `STALE — built from <a>, sources are now <b>`, clearable only by
    // `touch build.rs`. The same hazard is handled forty lines below for the
    // submodule gitlink (issue 0419), so this file already knew `.git` may be a
    // file — the worktree case was never swept in (the issue-0196 shape).
    //
    // The resolution lives in `source_stamp.rs` beside the code that READS the
    // index, and reuses its `git()`/`git_program()` helper rather than adding a
    // second way to run git — issue 0561's rule: one expression per stamp
    // input, shared with the runtime through the `include!` above.
    match git_index_path(&root) {
        Some(index) => println!("cargo:rerun-if-changed={}", index.display()),
        // LOUD, because this is the degraded mode issue 1336 asked to be made
        // visible: the stamp is baked but nothing will re-bake it. Silent only
        // when there is no repository at all (a tarball or vendored tree),
        // where `source_stamp()` also returns `None` and the freshness check
        // skips itself — so there is nothing to warn about.
        None if in_git_checkout(&root) => println!(
            "cargo:warning=nros-cli-core: git could not resolve `--git-path index` \
             (needs git >= 2.31); NROS_CLI_SOURCE_STAMP is UNWATCHED, so a commit \
             will not re-bake it and `just check cli-fresh` may report a stale CLI \
             that `just setup-cli` cannot clear — issue 1336."
        ),
        None => {}
    }

    let stamp = source_stamp(&root).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=NROS_CLI_SOURCE_STAMP={stamp}");

    // Issue 1018 — bake the stamp PER INPUT beside the fold, so the run-time
    // refusal can say WHICH input moved instead of inferring it.
    //
    // Without this the guard has one number and has to guess by elimination:
    // "no uncommitted CLI edits here, so your checkout must have moved" — which
    // is what a contributor who had done nothing but move a submodule pin was
    // told, and it is false. Elimination cannot be made correct, either: two
    // inputs can move at once, and the residual arm would then name only the
    // one it happened to check last.
    //
    // A flat `label=hash,label=hash` string rather than anything structured:
    // `rustc-env` carries text, the labels are `[A-Za-z_]`, and a parser in
    // `stale_guard` that can fail is one more thing between a contributor and
    // the sentence they need. A binary built before this exists simply has no
    // components, which `stale_guard` treats as "cannot attribute" and falls
    // back to the pre-1018 wording — never as "nothing moved".
    let components = source_stamp_components(&root)
        .map(|parts| {
            parts
                .into_iter()
                .map(|(label, hash)| format!("{label}={hash}"))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    println!("cargo:rustc-env=NROS_CLI_SOURCE_STAMP_COMPONENTS={components}");

    // issue 0409 — the play_launch pin this CLI was built against. `nros sync`
    // shells out to `nros-launch-resolve`, which stamps the SAME value from its
    // own build; a mismatch means the resolver was compiled from a different
    // layer-2 checkout, and such a resolver produces models that are missing
    // DATA rather than failing (one predating rlm v0.1.1 silently drops every
    // `params` projection). Crate versions cannot catch it — the two are
    // versioned in lockstep and read the same number.
    //
    // Read from the SUBMODULE working tree, not the superproject gitlink: what
    // the resolver compiled in is the commit actually checked out.
    let play_launch = root.join("packages/cli/third-party/play_launch");
    // issue 0419 — require a REPOSITORY, not a directory. An uninitialised
    // submodule is an empty directory that EXISTS, and `git -C <empty dir>
    // rev-parse HEAD` walks UP to the enclosing repo and returns the
    // SUPERPROJECT's HEAD. The pin then names a nano-ros commit, the 0409 guard
    // compares it against the resolver's real play_launch sha, and reports a
    // mismatch that cannot be true. A submodule checkout has a `.git` FILE, so
    // gate on that: an uninitialised one yields "unknown", which is what this
    // code always intended and which the guard treats as unverifiable.
    //
    // Re-stamp when it appears: without this rerun-if-changed,
    // `git submodule update --init` changes no input cargo watches, so build.rs
    // never re-runs and `setup-cli` reports success while rebuilding nothing —
    // the wrong pin is STICKY, and even `touch build.rs` does not clear it.
    // WHICH files must be watched is not obvious and got it wrong twice, so it
    // lives in one shared helper (`build-support/submodule_watch.rs`,
    // issue 0921) that this and the resolver's build.rs both `include!`. The
    // short version: the gitlink is a FILE whose content never changes, and
    // `<gitdir>/HEAD` only moves for a DETACHED submodule — on a branch the
    // commit moves the ref file instead. Getting it wrong makes the 0409 guard
    // compare a stale value against a fresh one.
    watch_submodule_commit(&play_launch);
    // Issue 0561 — ONE expression computes this, shared with `source_stamp()`
    // via the `include!` above. It used to be spelled out again here, which is
    // how the stamp came to watch something different from what the build baked:
    // the two agreed only by inspection, and then stopped. The 0419 gate (a
    // submodule needs a `.git` FILE, or `git -C` walks up to the superproject)
    // lives inside `play_launch_pin` now, so it cannot be forgotten at one site.
    let pin = play_launch_pin(&root).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=NROS_PLAY_LAUNCH_SHA={pin}");
}
