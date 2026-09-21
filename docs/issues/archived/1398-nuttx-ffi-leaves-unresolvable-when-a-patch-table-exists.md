---
id: 1398
title: "`check-leaf-lockfiles` reads a central patch table as proof the tree is
  provisioned, so an absent submodule reports as two broken nuttx leaves"
status: resolved
type: bug
area: gates, build, nuttx
related: [issue-0466, issue-1184, issue-1226, issue-0378]
---

## Problem

`check-leaf-lockfiles` resolves every tracked leaf with `cargo metadata
--locked` and sorts a failure into *drifted*, *unsynced* or *broken*. The
`unsynced` bucket then gets re-read:

```sh
if [ -f "nros-patch.toml" ]; then
    echo "ERROR: ${#unsynced[@]} leaf crate(s) cannot resolve on a SYNCED tree." >&2
    ...
    echo "       `nros-patch.toml` exists, so this is not the setup gap below —" >&2
    echo "       these leaves are genuinely unresolvable. Re-run `nros sync`; if" >&2
    echo "       they persist, their patch tables or `generated/` trees are wrong." >&2
    exit 1
fi
```

The inference is "the central patch table exists, therefore the tree is
provisioned, therefore an unresolvable leaf is a real defect". That does not
hold: `nros sync` writes patch tables and `generated/` trees, and it does not
fetch SUBMODULES. A tree can be fully synced and still have none checked out.

Two leaves path-depend on one:

```toml
# packages/boards/nros-board-nuttx-qemu/nros-nuttx-{ffi,riscv-ffi}/.cargo/config.toml
[patch.crates-io]
libc = { path = "../../../../third-party/nuttx/libc" }
```

That row is in the TRACKED half of the config (RFC-0048 W9 — authored, not
`nros sync`-managed), so it is there in a fresh clone. With
`third-party/nuttx/libc` not checked out, cargo says:

```text
error: failed to load source for dependency `libc`
Caused by:
  unable to update /…/third-party/nuttx/libc
```

which the gate classifies as `unsynced` (its `UNSYNCED_RE` matches `failed to
load source for dependency`) and then re-labels as "genuinely unresolvable …
their patch tables or `generated/` trees are wrong". Both halves of that
sentence are false, and the remedy it prescribes cannot work: `nros sync` does
not fetch submodules, and run at the repo root it refuses outright —
`sync: no src/<pkg>/package.xml and no package.xml at root … expected
colcon-style workspace or single-pkg dir`.

## Measured

On `origin/main`, no branch under test:

1. Fresh worktree, no `nros-patch.toml` → gate takes the setup-gap branch and
   passes. This is what CI and every fresh clone do, which is why nobody sees
   it (the shape of issue 1226: a gate that works is not a gate that runs).
2. `nros sync` any **Rust** workspace with `NROS_REPO_DIR` pointed at the
   checkout → writes the 806-byte central `nros-patch.toml`. (A C/C++ workspace
   does not: "sync: no Rust consumer pkgs — patch tables not written", so which
   workspace you synced last decides whether the gate checks at all.)
3. Re-run `just check leaf-lockfiles` → FAILS, naming
   `nros-nuttx-ffi` and `nros-nuttx-riscv-ffi`.
4. `git submodule update --init --depth 1 third-party/nuttx/libc`, re-run →
   **passes**. That is the whole cause; nothing about the leaves changed.

Blast radius is exactly those two leaves: they are the only tracked-lockfile
leaves whose manifest or cargo config points into `third-party/`.

## Why it matters

The verdict is confidently wrong in the direction that costs the most: a
contributor who has synced (so the gate now checks) but has not initialised the
nuttx submodules is told their patch tables are broken and sent to a command
that cannot help. `check-leaf-lockfiles` is on the fast line, so it also blocks
`pre-push` for a condition that is about the environment, not the tree — the
exact failure mode issue 0466 fixed for the no-patch-table case and left
standing for this one.

## Fix (not decided here)

- **Classify the submodule case on its own.** `unable to update <path under
  third-party/>` is a provisioning gap whatever the patch table says, because
  the patch table is evidence that `nros sync` ran and nothing else. Report it
  with the remedy that works (`git submodule update --init <path>`), and keep
  the hard failure for leaves that fail for any other reason.
- **Or check the path-dep targets directly.** The two rows are readable; a leaf
  whose `[patch.crates-io]` names a directory with no `Cargo.toml` is
  unprovisioned, and that answer does not depend on how cargo phrases its error.

Either way the guard belongs with the classification, not with the
patch-table test.

## Acceptance

With a central `nros-patch.toml` present and `third-party/nuttx/libc` NOT
checked out, `just check leaf-lockfiles` names the missing submodule and the
`git submodule update --init` remedy, rather than blaming the leaves' patch
tables. With the submodule checked out it stays green, and a leaf that is
genuinely broken on a synced tree still fails hard.

## Resolution

Fixed by classifying the provisioning gap on its own, **before** the
`unsynced` / `broken` split and independently of the patch-table test —
candidate two of the two the Fix section offered, with candidate one's remedy
text. `check-leaf-lockfiles` now reads each failing leaf's declared `path =`
targets (its `.cargo/config.toml` and its manifest, both resolved relative to
the leaf directory) and asks whether any of them is a DECLARED submodule
(`.gitmodules`) with no `Cargo.toml`. If so the leaf is NOT VERIFIED: recorded
through the `nros_check_skip` ledger (issue 1184), reported with
`git submodule update --init <path>`, exit 0 (issue 0466 — this gate is on the
`pre-push` fast line and must not refuse a push over a checkout the lane does
not provide). Issue 1043's three-outcome vocabulary, third series.

Reading the path deps rather than cargo's wording is the sturdier half: a
message is something upstream can reword, a missing `Cargo.toml` is not.
Requiring the target to be a declared submodule is what keeps the gate honest —
a `path =` pointing at a directory nobody can check out is a broken tree, not a
missing checkout, and still fails hard. Measured with the submodule present and
one extra `path` row added to `nros-nuttx-ffi`'s config: exit 1, naming that
leaf.

`nros-patch.toml` now means only what it can mean — `nros sync` ran here — and
the hard failure it guards is left for the sync-shaped gap it was written for.

The gate gained a selftest that runs on the NORMAL path (so it left
`.config/gate-selftest-baseline.txt`), covering both directions plus the third
that makes them non-vacuous: an absent submodule reads as a provisioning gap;
the same leaf with the target checked out does not; an absent path dep that is
NOT a declared submodule does not either. It is pure — directories and files,
no `git init`, so issue 0986 cannot apply. Each arm was confirmed live by
mutating the classifier and watching the selftest go red.
