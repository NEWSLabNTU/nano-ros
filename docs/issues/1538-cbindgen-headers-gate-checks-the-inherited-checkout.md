---
id: 1538
title: "`nros-cbindgen-headers` takes `NROS_REPO_DIR` env-first with no
  re-rooting, so in an agent worktree `just check cbindgen-headers` verifies the
  MAIN checkout's committed headers instead of the ones being edited"
status: open
type: bug
area: [tooling, build]
severity: medium
found: 2026-09-28
related: [1280, 1391, 1336, 1510, 0196]
---

## What happens

```
packages/tooling/nros-cbindgen-headers/src/main.rs:103
    let root = std::env::var("NROS_REPO_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root());
```

`root` is where the tool looks for every `HEADERS` entry's crate and committed
header. The env value wins outright, and `repo_root()` — derived from the
tool's own manifest path — is only the fallback.

A linked worktree inherits `NROS_REPO_DIR` pointing at the checkout that spawned
it (`activate.sh` exports it; the git status snapshot of any agent session shows
it set). So inside a worktree this tool reads the **main checkout's** crates and
compares them against the **main checkout's** committed headers, and reports OK
or STALE about a tree nobody is editing.

That is issue 1280's rule, and this site does not apply it: *an inherited
absolute path outranks the checkout you are building.* The discriminator is not
"is the variable set", it is where the value points — **outside any checkout →
KEEP, a DIFFERENT checkout → RE-ROOT here, this one → keep.**

## Why it matters

`check-cbindgen-headers` is the gate that keeps committed cbindgen output from
drifting from the Rust it is generated from. In a worktree it answers the wrong
question in the SAFE-LOOKING direction: a worktree that edits a `#[repr(C)]`
struct and forgets to regenerate gets an OK, because the main checkout is
consistent with itself.

Agent worktrees are how parallel work happens in this repo, so this is not a
corner: it is the normal shape for any session that touches an FFI surface. It
was found by a phase-456 W9 change that added an FFI entry point and had to pass
`NROS_REPO_DIR=<worktree>` by hand to get a meaningful verdict — a workaround
that happens to be the same variable, so nothing marks it as one.

## Not the same as issue 1510

`1510` fixed the rule for the workspace's own checkout resolution
(`fix(#1510): the workspace's own checkout outranks an inherited NROS_REPO_DIR`).
This is a second reader of the same variable that never got the rule — issue
0196's shape, a fix landed where the symptom was seen.

## What a fix looks like

Route through the one rule rather than re-deciding it here:
`nros_build_paths::reroot_foreign` is what every `build.rs` uses, and
`scripts/lib/checkout-paths.sh` is the shell side. "Which checkout" is the marker
walk, never `.git` — a worktree's `.git` is a FILE (issue 1336).

Note the crate is under `packages/tooling/`, so whether it can depend on
`nros-build-paths` needs checking rather than assuming; if it cannot, the walk
belongs in a shared helper both can reach, not copied.

## Acceptance

* Run inside a linked worktree with `NROS_REPO_DIR` inherited from the parent,
  the tool verifies the WORKTREE's headers.
* A genuinely out-of-tree `NROS_REPO_DIR` is still honoured (that is why
  env-first exists).
* A regression test that builds both checkout shapes rather than asserting the
  rule — the shape `check-inherited-checkout-paths` and
  `check-git-dir-layout-assumptions` already use, because a side-by-side-only
  probe is what let issue 1391's nesting case through.
* Whether any OTHER reader of `NROS_REPO_DIR` has the same gap is stated, since
  this is the second one found: `git grep -n 'NROS_REPO_DIR' -- '*.rs' '*.py' '*.sh'`.

## How it was measured

```sh
sed -n '100,106p' packages/tooling/nros-cbindgen-headers/src/main.rs
```

Reading only — the wrong-tree verdict was reported by a worktree session that had
to override the variable to get a real answer; this issue has not yet reproduced
the false OK end to end, which the acceptance test above is what would.
