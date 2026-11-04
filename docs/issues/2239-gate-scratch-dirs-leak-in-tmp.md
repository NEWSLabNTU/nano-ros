---
id: 2239
title: "Two build gates leave their scratch trees in /tmp, which fills on a developer host"
status: open
type: tech-debt
severity: medium
area: [ci, tooling]
related: [phase-489, issue-2223, issue-1353]
found: 2026-11-04
---

**Home: [phase-489](../roadmap/phase-489-ci-build-speedup.md)** — homed 2026-11-04: W8 chose `/tmp` for the gate's scratch copies.

## What

`check-template-copy-out` and `check-scaffold-builds` each build in a fresh
`mktemp -d "${TMPDIR:-/tmp}/nros-<gate>.XXXXXX"` tree, outside the checkout.
On a developer host those trees are never removed, so `/tmp` fills.

Measured on the maintainer's host on 2026-11-04:

- 79 `nros-template-copy-out.*` trees and 2 `nros-scaffold-builds.*` trees,
  about 15.5 GB in all. The oldest was from 2026-10-06, the largest 4.0 GB.
- `/tmp` is a 20 GB tmpfs (RAM-backed), so it sat at 16 GB used, 80 %.
- A Claude Code session writing a 3,278-line CI log into its own `/tmp`
  scratch folder failed with `EDQUOT`, and the command's output was lost.
- None of the trees was open by any process. Deleting them all brought `/tmp`
  to 885 MB.

## Why

Three paths keep a tree, and only one of them is meant to:

1. **A passing copy-out is kept unless CI asks.** `check-template-copy-out.sh`
   removes a passing tree only under `nros_gate_prune_enabled`
   (`scripts/build/gate-prune.sh`), which is `NROS_CI_PRUNE_GATE_OUTPUTS=1`.
   That is set in CI for issue 2223's disk budget, and nowhere locally, so
   every local pass keeps its 0.6 to 1 GB tree for good. `check-scaffold-builds`
   already removes its tree on a pass.
2. **A failing run keeps its tree in both gates.** This is deliberate: the tree
   and its `build.log` are the evidence (CLAUDE.md, never wipe a failing build).
   But nothing ever retires one, so each red adds a tree for good.
3. **An interrupted run keeps its tree.** Neither script sets a `trap` on
   `EXIT`/`INT`/`TERM`, so a killed gate, or one under a cancelled `just ci gate`,
   leaves a half-built tree that is neither a pass nor evidence of anything.

Issue 2223 already ruled that a passing gate's private tree is correct and merely
finished, so dropping it loses no evidence. That ruling was applied to CI only.

## What would close it

Choose by measurement; candidates:

- Drop a passing tree by default, on every host, in both gates. Keep the CI
  switch only if some lane really wants passed trees kept.
- Bound the failure evidence: keep the newest failing tree per gate (or per
  template), and remove older ones when a new run starts. The failure message
  already prints the tree's path, so the latest one is the one anybody reads.
- A `trap` that removes the tree of an interrupted run, unless the run had
  already reached a verdict worth keeping.
- Or build under the checkout's own `build/` (as other gates' private trees do),
  where `just gc-shared-cargo` and the disk tooling can see it, instead of in a
  RAM-backed `/tmp`. Phase-489 W8 chose `/tmp` for the copy so that no
  ancestor of the copy is the checkout (issue 1453); any move must keep that.

Acceptance: running `just check template-copy-out` and `just check
scaffold-builds` to a pass, to a fail, and interrupted leaves at most one tree
per gate (the newest failure) in `${TMPDIR:-/tmp}`.
