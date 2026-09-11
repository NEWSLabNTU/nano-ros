---
id: 1306
title: "In a linked git worktree the `nros` CLI's source stamp never re-runs on a
  commit: `build.rs` watches `<root>/.git/index`, which is not a file there"
status: open
type: bug
area: cli, build
severity: low
related: [issue-0466, issue-0627, issue-1285]
---

## What happens

`packages/cli/nros-cli-core/build.rs` stamps the CLI with a hash of its sources
(`NROS_CLI_SOURCE_STAMP`), and `source_stamp` reads index blob SHAs. So the
stamp must be recomputed whenever the index moves (commit, rebase, branch
switch), and build.rs registers the index for that:

```rust
let index = root.join(".git/index");
if index.exists() {
    println!("cargo:rerun-if-changed={}", index.display());
}
```

In a LINKED worktree (`git worktree add`, which is how every parallel agent
session here works), `<root>/.git` is a FILE (`gitdir: …/.git/worktrees/<name>`)
and the worktree's real index is `<main>/.git/worktrees/<name>/index`. So
`root.join(".git/index")` does not exist, the `if` is false, and nothing watches
the index at all.

Result: after a commit in a worktree, the source files are unchanged on disk, so
none of the `rerun-if-changed` paths fire, build.rs does not re-run, and the
baked-in stamp is the pre-commit one. `nros source-stamp` / `just check
cli-fresh` then report the CLI stale against the tree, and `just setup-cli`
rebuilding does not help, because cargo sees nothing to rebuild.

## How it was found

Issue 1285's agent (2026-09-11) hit `CI GATE FAILED at step 1 of 6. FAILED
check::cli-fresh` in its worktree right after committing. It confirmed the cause
by touching `build.rs`, which forced a re-run, after which the stamp read fresh.
The agent working on PR #919 (pure-C ThreadX) then needed the same workaround. The main checkout is
unaffected, because there `.git` is a directory.

## Fix

Resolve the index through git rather than by joining a path:
`git rev-parse --git-path index` (run in `root`) returns the right file in a
main checkout, in a linked worktree, and under `GIT_INDEX_FILE`. Watch that path.
If it cannot be resolved, fall back to watching nothing and say so with a
`cargo:warning`, so the fallback is visible and does not look like a stamp that
is fresh. Check the other `.git/`-joined paths in build scripts for the same
shape (`rg -n '"\.git/' --glob build.rs`) and fix them together.

## Acceptance

In a linked worktree: build the CLI, commit a change to a CLI source file,
revert the source file's content only (so the tree is back to the pre-commit
content, but the index SHAs moved), and `just setup-cli` then `nros source-stamp`
must read fresh with no manual `touch`. Show the same run failing before the fix.
