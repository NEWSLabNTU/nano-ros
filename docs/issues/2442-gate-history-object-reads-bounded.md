---
id: 2442
title: "A gate that reads old trees or blobs in CI's treeless history fetches them one object at a time, unbounded"
status: open
type: bug
severity: high
area: [ci, gates, git]
found: 2026-11-11
related: [phase-543, issue-1476, issue-1043, issue-0650]
---

**Home: [phase-543](../roadmap/phase-543-nextest-gates.md)** (gate lanes as scopes, the harness and its provisioning).

## Measured

CI lanes that claim `git-history` provision it with
`git fetch --filter=tree:0 --unshallow` (`scripts/ci/provision-gate-lane.sh`).
After that, `git rev-parse --is-shallow-repository` is `false`, so the
truncation guard of issue 1476 (`scripts/lib/git_history.py`, `is_truncated`)
lets the clone through. But the clone is PARTIAL: every commit is whole, and
every tree and blob below the fetched tips lives on the promisor remote. A git
command that needs one fetches it lazily, one object per round trip, from
inside the command.

`check-toolchain-paths-persist` (PR #3515, first version) ran
`git log --diff-filter=DR --name-status -- cmake/toolchain/` there. In CI's
`check / fast` it ran into the 40-minute nextest cap. In a reproduced clone it
was still walking when `timeout 90` killed it. #3515's next version reported
the partial clone NOT VERIFIED, and the gate harness (`NROS_CHECK_SKIP_STRICT=1`)
read that as red (run 42950969291: 547 passed, 1 failed). #3515 as merged
compares against one baseline list instead and walks nothing.

Commit-only questions (`merge-base --is-ancestor`, `rev-list`, `log -1
--format=%s`, `cat-file -e <sha>^{commit}`) are fine in a treeless clone and
are not this issue.

## The provide hole

`scripts/ci/pin-baseline.sh provide` fetched the baseline only when
`cat-file -e <sha>^{commit}` failed. In a treeless clone the commit is present
without its tree, so the fetch was skipped and `ls-tree -r <baseline>`
(check-submodule-pins) walked the network tree by tree. Re-fetching a commit
already present does not help (measured on a local `file://` partial clone:
plain, `fetch.negotiationAlgorithm=noop` and `--depth=1` all left the root
tree missing, and `--depth=1` made the repository shallow again). Fetching the
root TREE by id with `--filter=blob:none` brings every subtree at once.

Also measured, NOT fixed here: `provide`'s first step, the depth-1 fetch of
the base BRANCH, makes an already unshallowed clone shallow again. In CI it is
harmless because `provide` runs before `provision-gate-lane.sh`, which
unshallows whenever the clone is shallow. It would matter only in a workspace
that ran the two steps in the other order.

A lazy fetch of ONE tree pulls that whole tree (measured: 0.9 s and about
0.6 MiB for one old commit of this repository). What made the history walk
unbounded was one tree per COMMIT.

## Sweep

Command: `python3 scripts/check/check-history-object-reads.py` (the gate this
issue adds). Before the fix it reported exactly the seven EXPOSED or latent
sites below. Each of the others was read by hand.

| gate (tier) | git call | lanes | verdict | now |
| --- | --- | --- | --- | --- |
| toolchain-paths-persist (fast) | `log --diff-filter=DR --name-status -- cmake/toolchain/` (first version); `show <base>:<list>` (merged) | fast, build, queue (treeless) | EXPOSED (walk), then one path at one commit | `history_git(..., fetch=True)`: two trees and a blob, bounded |
| unsafe-census (fast) | `diff --name-only <merge-base origin/main HEAD> --` | fast | SAFE-BY-LANE (provide fetches that base with its tree), unbounded otherwise | `tree_present` first; absent or timeout -> every crate enforced (fail closed) |
| submodule-pins (fast), `submodule-pins-check.sh` | `ls-tree -r <baseline>` | fast | SAFE-BY-LANE, latent (the provide hole) | `nros_git_tree_present` first: absent -> FAILED in CI, NOT CHECKED locally (its existing baseline arm); bounded read, a failed read FAILS, never an empty pin set |
| `pin-baseline.sh` (CI steps provide, init-moved, deepen) | `ls-tree -r <baseline>`; provide's commit-only presence test | gate.yml, host-tests, run-matrix | latent (the provide hole) | provide tests the TREE and fetches the root tree by id; `_nros_pin_gitlinks` bounded |
| submodule-commits-reachable (exempt; pre-push `--changed`) | `ls-tree <ref> <path>` | pre-push (full clone) | SAFE-BY-LANE | bounded; a failed read checks the pin (fail closed) |
| source-refs (fast) | `ls-tree <rev>`, `show <rev>:nros-sdk-index.toml` | the gate passes no `--rev`; `stage-sdk-root.sh` passes HEAD | SAFE | `--rev` reads bounded behind `tree_present` (NOT VERIFIED, exit 2) |
| roadmap-commit-refs, doc-commit-citations (needs:git-history) | `log -1 --format=%s`, `rev-list`, `cat-file -e ^{commit}` | fast | SAFE (commit-only) | unchanged |
| hook-repo-side-effects, pristine-checkout, gate-lane.sh, submodule-provision, submodule-drift, submodule-pinned-locks, workflow-checkout-before-repo-path, stage-sdk-root | `diff HEAD`, `ls-tree HEAD`, `grep ... HEAD`, submodule `log a..b --oneline` | various | SAFE (HEAD, index, or commit-only) | unchanged |
| not gates: audit-nets.py, fixture-tree.sh, run-bootstrap-probe.sh; pin-baseline's `cat-file -p <commit>` | `diff <since>`; `diff <a> <b>` in the fixture repo; `show <tip>:<file>` (fetch-depth: 0); a commit object | n/a | SAFE, in the gate's EXEMPT with reasons | unchanged |

## Fix

- `scripts/lib/git_history.py`: `is_partial`, `history_objects_unreadable`,
  `tree_present` (asked with `rev-list --missing=print`, which reports instead
  of fetching, on git 2.34 too), `history_git` (no prompt,
  `GIT_NO_LAZY_FETCH=1` unless `fetch=True`, stdin closed, a timeout raising
  `HistoryUnknown`). Shell twin `scripts/lib/git-history.sh`:
  `nros_git_partial`, `nros_git_history_objects_unreadable`,
  `nros_git_tree_present`, `nros_git_history_run`. Both self-test against a
  full, a shallow and a CI-shaped partial clone built under `$TMPDIR`.
- Gate `check-history-object-reads` (fast tier): refuses a raw read of old
  trees or blobs outside the helper, with an EXEMPT table of written reasons.
- The sites above moved onto the helper. The provide hole is closed.
