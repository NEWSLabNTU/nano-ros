---
id: 1398
title: "`check-leaf-lockfiles` reads a central patch table as proof the tree is
  provisioned, so an absent submodule reports as two broken nuttx leaves"
status: open
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
