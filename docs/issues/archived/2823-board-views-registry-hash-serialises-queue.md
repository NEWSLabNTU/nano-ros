---
id: 2823
title: "The board-support views stamp a registry digest, so any two registry edits conflict and the merge queue serialises them"
status: resolved
type: tech-debt
severity: medium
area: [ci, tooling, boards]
related: [issue-0883, issue-0884]
found: 2026-11-22
resolved: 2026-11-22
---

## What

`scripts/gen-board-support.py` writes 43 views from the board-support registry
(`packages/boards/board-support.toml` plus its cell table). Five of them
carry a stamp on their first line:

- `book/src/reference/board-support-levels.md`
- `packages/boards/board-support-status.json`
- `packages/cli/nros-cli-core/src/cmd/probe_boot_rows.rs`
- `packages/testing/nros-tests/src/matrix_cells.rs`
- `scripts/probe/probe-tracks.json`

The stamp included `(registry data <digest>)`, a hash over the whole
registry. So any edit to any row rewrote that line in all five files. Two pull
requests that edited DIFFERENT rows therefore conflicted on all five, and so
did a rebase onto a `main` that had taken any registry edit.

On 2026-11-22 this cost three PRs four rebases (#4506 twice, #4524 twice).
Each time the only conflicts were the five stamp lines, and once a queued full
gate would have run on a head that could no longer merge. It is issue 0883's
shape: a committed generated file that many PRs touch serialises the queue.
GitHub rebases queue entries on the server, so no local merge driver helps.

## Why the digest was not needed

Staleness never read it. `--check` renders every view and compares the WHOLE
text (`stale_views`), so a stale view is caught by its content whatever its
header says. The digest only ever told a reader which registry state wrote the
file, and the generator and `check-board-registry.py` still print it on
their own output.

## Fix

`Registry.stamp` no longer includes the digest. The five views were
regenerated once, the last time every in-flight registry PR conflicts on them.

Measured with `git merge-tree`: two branches, each editing a different
registry row's `notes` and regenerating:

| stamp | views each side rewrote | conflict markers |
|---|---|---|
| with digest (`main` before this) | 5 and 5 | 5 |
| without digest | 1 and 0 | 0 |

The first row is the negative control. With the digest, the side whose edit
renders nowhere still rewrote all five views, on their first line only.

Not done: gitignoring the views (issue 0884's fix for `open.md`). These
views are committed for readers without a build, and the book and the test
crate compile from two of them.

## Remaining

Two PRs that edit rows rendering into the same lines of one view still
conflict. That is a real conflict between their contents, not churn.
