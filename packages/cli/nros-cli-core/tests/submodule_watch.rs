//! Issue 0921 — the watch set for a submodule's checked-out commit.
//!
//! The helper is `include!`d by two build scripts, which nothing else compiles,
//! so without this file it has no test at all. That is how the bug survived:
//! the rule was expressed only as a `rerun-if-changed` line, and a watch that
//! names the wrong file is indistinguishable from one that names the right file
//! until a commit lands in exactly the wrong place.
//!
//! No repo is created here. The paths are derived from `.git` and `HEAD`
//! CONTENT, so a few files in a temp dir reproduce both shapes exactly, and the
//! test says what it is really about — which path a commit moves — rather than
//! exercising git.

use std::path::{Path, PathBuf};

include!("../../build-support/submodule_watch.rs");

/// A fake submodule: `<root>/sub/.git` -> `<root>/modules/sub`, with `HEAD`
/// holding `head_content`.
fn fake_submodule(root: &Path, head_content: &str) -> PathBuf {
    let sub = root.join("sub");
    let gitdir = root.join("modules").join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::create_dir_all(gitdir.join("refs").join("heads")).unwrap();
    // A RELATIVE gitdir, which is what `git submodule` actually writes.
    std::fs::write(sub.join(".git"), "gitdir: ../modules/sub\n").unwrap();
    std::fs::write(gitdir.join("HEAD"), head_content).unwrap();
    sub
}

#[test]
fn a_branch_checkout_watches_the_ref_the_commit_moves() {
    // THE regression. `HEAD` holds `ref: …` and is constant across commits on
    // that branch, so watching it alone never fires — which is the state a
    // submodule is in exactly while someone is developing a change to it.
    let tmp = tempfile::tempdir().unwrap();
    let sub = fake_submodule(tmp.path(), "ref: refs/heads/my-work\n");
    let paths = submodule_watch_paths(&sub);
    let gitdir = tmp.path().join("modules").join("sub");

    assert!(
        paths.contains(&gitdir.join("refs").join("heads").join("my-work")),
        "the ref a commit on `my-work` moves is not watched, so the build \
         script cannot re-run: {paths:?}"
    );
    assert!(
        paths.contains(&gitdir.join("packed-refs")),
        "a packed ref has no loose file, so the loose watch would be inert: \
         {paths:?}"
    );
    assert!(
        paths.contains(&gitdir.join("logs").join("HEAD")),
        "the reflog covers commit AND checkout in one file: {paths:?}"
    );
}

#[test]
fn a_detached_checkout_still_watches_head() {
    // The case that always worked, kept so a later tidy-up cannot trade one
    // shape for the other. Detached `HEAD` holds the sha itself, so the file
    // is rewritten on every pin bump.
    let tmp = tempfile::tempdir().unwrap();
    let sub = fake_submodule(tmp.path(), "1234567890abcdef1234567890abcdef12345678\n");
    let paths = submodule_watch_paths(&sub);
    let gitdir = tmp.path().join("modules").join("sub");

    assert!(paths.contains(&gitdir.join("HEAD")), "{paths:?}");
    // No symref to follow, so nothing under refs/heads is named.
    assert!(
        !paths
            .iter()
            .any(|p| p.starts_with(gitdir.join("refs").join("heads"))),
        "a detached HEAD points at no branch, so no ref file should be \
         invented: {paths:?}"
    );
}

#[test]
fn an_uninitialised_submodule_still_watches_the_gitlink() {
    // `git submodule update --init` changes no input otherwise, so the stamp
    // would stay `unknown` forever while the recipe reported success. Cargo
    // treats an absent watched path as "re-run when it appears", which is what
    // makes this work.
    let tmp = tempfile::tempdir().unwrap();
    let sub = tmp.path().join("empty");
    std::fs::create_dir_all(&sub).unwrap();
    let paths = submodule_watch_paths(&sub);
    assert_eq!(
        paths,
        vec![sub.join(".git")],
        "an empty submodule dir has only the gitlink to wait for: {paths:?}"
    );
}

#[test]
fn a_plain_repo_is_not_mistaken_for_a_submodule() {
    // `.git` as a DIRECTORY is an ordinary checkout, whose refs sit under it
    // directly rather than behind a `gitdir:` redirect. Reading it as a file
    // fails, and treating that failure as "give up" would silently drop the
    // watch for anyone building outside the submodule layout.
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("plain");
    std::fs::create_dir_all(repo.join(".git").join("refs").join("heads")).unwrap();
    std::fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
    let paths = submodule_watch_paths(&repo);
    assert!(
        paths.contains(&repo.join(".git").join("refs").join("heads").join("main")),
        "{paths:?}"
    );
}
