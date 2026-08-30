// Which files must be watched for a SUBMODULE's checked-out commit to
// invalidate a build script (issue 0921).
//
// `include!`d by `nros-cli-core/build.rs` and `nros-launch-resolve/build.rs`.
// Both bake the play_launch commit into their binary and the 0409 guard
// compares the two, so a stale value in either makes the guard compare a lie.
// CLAUDE.md's rule applies literally: ONE shared helper, never a second
// spelling — this is the third time these two files have been edited in step.
//
// Written for `include!`, so: no inner doc comments (`//!` is only legal at the
// top of a file) and no `use` (it would collide with the includer's imports).
// Types are spelled out for the same reason.
//
// ## Why `<gitdir>/HEAD` alone is not enough
//
// It was the whole answer until issue 0921, on the reasoning that "the move
// happens in `<gitdir>/HEAD`". That holds for a DETACHED submodule, which is
// the normal state: `HEAD` holds the SHA, so a pin bump rewrites the file.
//
// It fails when the submodule is on a BRANCH. There `HEAD` holds
// `ref: refs/heads/<branch>` and does not change across commits — the commit
// moves `<gitdir>/refs/heads/<branch>`, or `packed-refs` if the ref is packed.
// The build script never re-runs, the binary keeps the previous SHA, and
// nothing announces it: `just setup-launch-resolve` prints `built:` either way,
// so the disagreement surfaces later in `nros sync` naming neither the branch
// nor the cause. Deleting the binary does not help — cargo still has no
// invalidated input, so it bakes the same stale value again.
//
// And the blind case is the one that matters: a submodule sits on a branch
// precisely while someone is developing a change to it, which is when its
// commit moves most often.

// Resolve `a/b/../c` to `a/c` WITHOUT touching the filesystem.
//
// `git submodule` writes a relative `gitdir:` (`../../.git/modules/<name>`), so
// every path below would otherwise carry the `..` hops. cargo resolves them
// fine, but these paths are also printed into build logs and compared in tests,
// and `sub/../modules/sub/refs/heads/my-work` is materially harder to read than
// `modules/sub/refs/heads/my-work`. Lexical, not `canonicalize`, because most
// of these paths deliberately do not exist yet.
#[allow(dead_code)]
fn lexically_normalize(p: &std::path::Path) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                // Only pop a real name; `../..` above the root must survive.
                let popped = out
                    .components()
                    .next_back()
                    .is_some_and(|last| matches!(last, std::path::Component::Normal(_)));
                if popped {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

// Every path whose change means the submodule's checked-out commit may have
// moved. Pure, so `tests/submodule_watch.rs` can check it without a build.
//
// Paths that do not exist are returned anyway, deliberately: cargo treats an
// absent watched path as "re-run when it appears", which is what re-stamps a
// binary built against an uninitialised submodule once `git submodule update
// --init` populates it.
#[allow(dead_code)] // the test binary uses this; each build script uses the wrapper
fn submodule_watch_paths(submodule: &std::path::Path) -> Vec<std::path::PathBuf> {
    let gitlink = submodule.join(".git");
    let mut paths = vec![gitlink.clone()];

    // A submodule's `.git` is a FILE holding `gitdir: <path>`. A plain
    // directory is an ordinary checkout, whose refs live under `.git` itself.
    let gitdir = match std::fs::read_to_string(&gitlink) {
        Ok(text) => match text.strip_prefix("gitdir:").map(str::trim) {
            Some(rel) => submodule.join(rel),
            None => return paths,
        },
        Err(_) if gitlink.is_dir() => gitlink.clone(),
        Err(_) => return paths,
    };

    let head = gitdir.join("HEAD");
    paths.push(head.clone());

    // Follow the symref. This is the line issue 0921 was missing: on a branch
    // the ref file is the only thing a commit touches.
    if let Ok(text) = std::fs::read_to_string(&head)
        && let Some(r) = text.strip_prefix("ref:").map(str::trim)
        && !r.is_empty()
    {
        paths.push(gitdir.join(r));
    }

    // A packed ref has no loose file, so the loose path above would never
    // appear and that watch would be inert.
    paths.push(gitdir.join("packed-refs"));

    // Appended on every commit AND every checkout, so it covers both shapes.
    // A belt, not a brace: reflogs are the default for a non-bare repo but can
    // be disabled, which is why the specific refs above are watched too.
    paths.push(gitdir.join("logs").join("HEAD"));

    paths.iter().map(|p| lexically_normalize(p)).collect()
}

// Emit the `cargo:rerun-if-changed` lines for `submodule_watch_paths`.
#[allow(dead_code)] // each build script uses it; the test binary does not
fn watch_submodule_commit(submodule: &std::path::Path) {
    for p in submodule_watch_paths(submodule) {
        println!("cargo:rerun-if-changed={}", p.display());
    }
}
