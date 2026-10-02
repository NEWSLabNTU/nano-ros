---
id: 1538
title: "`nros-cbindgen-headers` takes `NROS_REPO_DIR` env-first with no
  re-rooting, so in an agent worktree `just check cbindgen-headers` verifies the
  MAIN checkout's committed headers instead of the ones being edited"
status: resolved
type: bug
area: [tooling, build]
severity: medium
found: 2026-09-28
related: [1280, 1391, 1336, 1510, 0196, 1641]
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

## Fix — 2026-10-02

The root goes through `nros_build_paths::reroot_foreign`, the issue-1280 rule,
rather than a second spelling of it: a value outside every checkout is kept, a
value inside this checkout is kept, and a value inside a DIFFERENT checkout is
re-rooted onto this one, with a line on stderr saying so. `nros-build-paths` was
already in this crate's graph through `nros-build-helpers`, so the direct edge
costs the root lock one line and moves no version (recorded with `just
lock-update`). The crate CAN reach it, which the filing left open.

## Verified, end to end — the filing's own gap closed

This issue was filed from reading only and said so. It is now reproduced, in the
NESTED shape (`<main>/.claude/worktrees/e2e-1538`), which is the hard case:

```
worktree at origin/main (pre-fix), its nros_cpp_ffi.h edited to be STALE,
NROS_REPO_DIR=<main> inherited:
    check-cbindgen-headers: OK (3 committed headers match a fresh generation)   rc=0

same worktree, same stale edit, checked out at the fix:
    $NROS_REPO_DIR named another nano-ros checkout (<main>); using this one (<wt>)
    [FAIL] these committed headers are STALE against their crate sources:
             <wt>/packages/api/nros-cpp/include/nros/nros_cpp_ffi.h            rc=1
```

The false OK is real, and the fix turns it into the failure it should have been.

Unit tests build both checkout shapes on disk (sibling and nested) plus the two
cases that must not change (outside every checkout; this checkout).
Mutation-checked: reverting to env-first fails exactly the sibling and nested
tests and leaves the two keep-cases passing.

## The sweep — answered, and it is not small

`git grep -n 'NROS_REPO_DIR' -- '*.rs'` finds **seven more raw readers**, all
CLI-side, none going through issue 1510's resolver. Two are worse than this
issue — one WRITES into the root it picks, one emits path deps that would compile
the parent's core crates. Filed as **issue 1641** with a per-site table rather
than folded in here: they span four crates and need a decision each.
