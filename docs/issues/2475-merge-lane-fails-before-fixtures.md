---
id: 2475
title: "The `merge` lane's hosted shards fail before building a fixture: an unprovisioned rosidl clone, three unprovisioned build sources, no pinned nextest in the zephyr-ci image, and 26-49 min of tree-wide codegen plus `check fast` inside `just build merge`"
status: open
type: bug
area: [ci, testing, tooling]
severity: medium
found: 2026-11-12
related: [2462, phase-535]
---

**Home: [phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md) W8.2.**
Fix this before `merge-runtime.yml` (dispatch-only since the 2026-11-12
ruling) re-measures what a per-merge cadence would cost for the host witness
boards. Until then no measurement can produce a sample, because no shard
reaches a test.

## Measured

Two runs of `merge-runtime.yml` from a temporary branch whose only change
was a `push:` trigger (job start to end, excluding queue wait; GitHub-hosted
`ubuntu-22.04`, `nano-ros-ci:humble` and `nano-ros-zephyr-ci:humble-sdk0.17.4-r6`):

| run | shard | time | failed in |
| --- | --- | --- | --- |
| 43173875037 | linux | 39.7 min | `just build merge`: no `NROS_GATE_SCOPE` on the job (fixed in #3665: `[lane.merge]`) |
| 43173875037 | threadx_linux | 37.2 min | same |
| 43173875037 | zephyr | 57.2 min | same |
| 43181966650 | linux | 50.2 min | `check fast` inside `just build merge`: `python-entry-provenance` NOT VERIFIED, no clone of `[source.rosidl]` under `$NROS_HOME/sources` |
| 43181966650 | threadx_linux | 59.7 min | `check fast` passed (540/540, 13.3 min); then `_require-build-sources`: `micro-xrce-dds-client`, `micro-cdr`, `px4-autopilot` not provisioned |
| 43181966650 | zephyr | 41.2 min | the `merge` lane preflight: the pinned cargo-nextest is not installed in the zephyr-ci image |

In run 43181966650, the steps before `just build merge` took 10-15 min per
shard (CLI build about 4 min, `just setup <board>` 2-3 min, disk reclaim about
1.5 min). Inside `just build merge`, the tree-wide `_codegen` and the
`check::fast` prerequisite of `build-test-fixtures` took 26-49 min before any
fixture work. The 20-minute p95 RFC-0122 D9 sets for a per-merge lane was
already exceeded 2-3 times before a fixture was built.

## Three defects, one cost

1. `[lane.merge]` claims `sources`, and `python-entry-provenance` is in scope
   under that claim, but the job provisions only what `just setup <board>`
   and `nros setup --source px4-rs --source nuttx-libc` fetch: no `rosidl`.
   Either drop the claim (the gate then reports itself out of scope by name)
   or provision the clone.
2. `build-test-fixtures` requires the index's build sources
   (`_require-build-sources`). The job never runs `nros setup --build-sources`.
3. The zephyr-ci image carries no pinned cargo-nextest, and the lane
   preflight refuses (`just setup-worktree` provides it on a developer host).
4. The cost: `just build <lane>` runs `_codegen` for every workspace in the
   tree and `check::fast` before the first fixture, whatever the lane. On a
   4-vCPU hosted runner that is most of the budget. A per-board shard should
   generate only its own coordinates' trees, and the gates already run in
   `gate.yml` on the same commit.

## Done when

A dispatch of `merge-runtime.yml` reaches `just ci merge` on all three shards,
and `scripts/ci/merge-runtime-p95.py` reports samples.
