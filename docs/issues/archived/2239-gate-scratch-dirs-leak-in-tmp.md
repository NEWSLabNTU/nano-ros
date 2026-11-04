---
id: 2239
title: "Two build gates leave their scratch trees in /tmp, which fills on a developer host"
status: resolved
resolved_in: "2026-11-05"
type: tech-debt
severity: medium
area: [ci, tooling]
related: [phase-489, issue-2223, issue-1353, issue-2254]
found: 2026-11-04
---

**Home: [phase-489](../../roadmap/phase-489-ci-build-speedup.md)** — homed 2026-11-04: W8 chose `/tmp` for the gate's scratch copies.

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

## Resolution (2026-11-05)

Both gates now take their scratch trees from one helper,
`scripts/build/gate-scratch.sh`, and the rule is the same in each:

| the run | its tree |
| --- | --- |
| passed | removed as soon as the gate has its verdict (as before) |
| failed | kept as the evidence the failure message points at, until the next run of the same gate and scope starts, or CI releases it (`NROS_CI_PRUNE_GATE_OUTPUTS`, unchanged) |
| interrupted | removed on the way out by an EXIT trap; after a SIGKILL, which no trap sees, removed by the next run of the same gate |

Every tree carries a `.nros-gate-scratch` marker naming its gate, its scope
(the arguments the gate ran with, so a `--part zephyr` failure is not retired
by a `--part no-zephyr` run), its owner's pid and that pid's start time. A run
never removes a tree whose owner is still alive, so two worktrees running one
gate at once do not touch each other's trees. A tree with no marker predates
this change and is left alone, because its owner cannot be proved dead.

The trees stay outside the checkout, which keeps issue 1453's requirement that
no ancestor of a copy is the checkout.

**Measured** with `TMPDIR` set to an empty directory outside any checkout (to
stay clear of issue 2240), on this host:

| step | trees left |
| --- | --- |
| `check-scaffold-builds rust_component_talker`, pass | 0 |
| `check-template-copy-out local-msg-package`, pass | 0 |
| the same two, failing twice each (a CLI wrapper failing `configure`/`build`) | 1 per gate, the newest failure |
| `check-template-copy-out multi-node-workspace`, SIGKILL mid-build | the killed run's tree |
| the next `check-template-copy-out` run | the orphan and the superseded failure are gone |

**One limit, measured, not fixed.** Ctrl-C mid-build signals the whole process
group. `nros build` catches SIGINT and exits normally, and bash's "wait and
cooperative exit" rule then skips the gate's own INT trap. So the gate reads
the interrupted build as an ordinary build failure and keeps that tree as
failure evidence. It is still retired by the next run of the same scope, so
the bound holds: at most the newest run's trees per gate and scope.

**Negative controls.** `just check gate-scratch` (fast line) plants trees for
each case and asserts which survive: an orphan and an earlier same-scope
failure go; a live run's tree, another scope's failure, another gate's tree
and an unmarked tree stay; on exit an unverdicted tree goes and a failure
stays; SIGINT removes the tree. Mutating the liveness check, the exit trap's
failure exemption or the EXIT trap itself each turns the control red.

**Not covered here.** The 50 unmarked trees (about 2 GB) already in `/tmp` on
this host predate the marker, so the gates leave them; remove them by hand
once. The same test also found `nros build`'s own jobserver FIFO directory
(`nros-jobserver-<pid>-<seq>`) surviving an interrupted build. That is filed
as [issue 2254](../2254-nros-build-jobserver-fifo-dir-leaks-on-signal.md).
