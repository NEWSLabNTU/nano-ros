---
id: 1234
title: "A linked worktree inherits `NROS_REPO_DIR` and writes its build dir and SKIP LEDGER into the parent checkout — one tree's skips appear in another's gate summary"
status: open
type: bug
area: ci, tooling
severity: medium
found: 2026-09-08
related: [1161, 1184, 0584, phase-449]
---

# Two checkouts, one skip ledger

Running `just check fast` in the main checkout while an agent ran the same lane
in a linked worktree, the main run's summary reported skips from the OTHER tree:

```
check-fast (parallel): 278 gate(s) ran, 5 SKIPPED
  [SKIPPED] workspace-order: no in-tree nros at
      /home/aeon/repos/nano-ros/.claude/worktrees/agent-af1ce245.../packages/cli/target/release/nros
  [SKIPPED] zpico-config-keys: zenoh-pico submodule not checked out
```

Neither is true of the checkout that printed them. `zpico-config-keys` skipped
because the WORKTREE has no submodule; the main tree does. An earlier run showed
`ivc-fsp` twice — once per tree.

## The mechanism, and it is documented in the function that has it

`scripts/build/build-root.sh:56-62`:

```sh
# worktree, where an inherited `NROS_REPO_DIR` may still name the main checkout.
nros_build_root() {
    if [ -n "${NROS_BUILD_ROOT:-}" ]; then
        printf '%s' "${NROS_BUILD_ROOT%/}"
        return 0
    fi
    printf '%s/build' "${NROS_REPO_ROOT:-${NROS_REPO_DIR:-$_NROS_BUILD_ROOT_REPO}}"
}
```

The last-resort `_NROS_BUILD_ROOT_REPO` is derived from `${BASH_SOURCE[0]}`, so
it WOULD resolve per-worktree and be correct. It never gets the chance:
`activate.sh` exports `NROS_REPO_DIR`, a child process inherits it, and the
middle branch wins. Measured: `NROS_REPO_DIR=/home/aeon/repos/nano-ros` in the
parent shell; the worktree has no `build/check-skips` of its own, and the
parent's `checks.skipped` carries a line naming the worktree's path.

The comment above the function names this case exactly. It reads as a caveat
that something downstream handles; nothing does.

## Why it matters beyond noise

The skip ledger is what `check-fast` prints as "N SKIPPED", and issues 0584,
1161 and 1184 all turn on that number meaning something. A skip that crossed
from another checkout is worse than an uncounted one: it is a gate reporting a
condition that is FALSE HERE. A reader chasing "zenoh-pico submodule not checked
out" in a tree where it plainly is has been sent somewhere that does not exist.

It is not only the ledger. Everything under `nros_build_root()` is shared —
fixture stamps, `cargo-fixtures/*` target dirs, the reconfigure snapshots. Two
trees building different branches into one `build/` is the phase-340 cargo-group
hazard (issue 0616) across checkouts rather than within one.

## What it does NOT appear to be

The verdicts themselves were not wrong in the runs observed: no gate reported a
false PASS or FAIL, and `rc` matched the tree it ran in. The corruption seen so
far is confined to the SKIP accounting and to shared build artifacts. That is a
statement about what was observed, not a proof — two trees writing one fixture
stamp is exactly the shape that produces a stale-artifact verdict, and the
`.fixtures-built` stamp lives under the same root.

## Shape of a fix

`NROS_REPO_DIR` should not out-rank the file's own location when the two
disagree. `_NROS_BUILD_ROOT_REPO` already computes the right answer from
`BASH_SOURCE`; the ordering is what defeats it. A worktree is detectable
(`git rev-parse --git-common-dir` differs from `--git-dir`), so the rule could
be: an inherited repo dir that is not an ancestor of this file's own root is
stale and ignored.

Whatever the fix, it wants a test — two checkouts is a shape no gate models
today, which is why a documented hazard survived as a comment.

## Reproduction

```sh
git worktree add /tmp/wt origin/main
source ./activate.sh          # exports NROS_REPO_DIR=<main checkout>
( cd /tmp/wt && just check fast )   # its skips land in the MAIN tree's ledger
grep agent /home/aeon/repos/nano-ros/build/check-skips/checks.skipped
```
