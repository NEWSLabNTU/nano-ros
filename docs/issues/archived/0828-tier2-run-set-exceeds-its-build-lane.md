---
id: 828
title: "Tier 2 RUNS rows its build lane never builds, so `just ci-matrix` is
  green only while an earlier `lane=all` build is still fresh"
status: resolved
type: bug
area: testing
related: [issue-0482, issue-0517, phase-340, phase-383]
---

## Problem

CLAUDE.md states the contract plainly: *"`just build-test-fixtures lane=tier2`
is the build it needs (phase-340 W3): the RUN narrows to the same coordinates at
fixture RESOLUTION time, so an out-of-lane fixture SKIPS rather than failing."*

That is true only for rows the resolver can **attribute**. Issue 0517 already
recorded the exception: a leaf whose `<dir>/<build_subdir>` is shared by several
rows is ambiguous by construction, and the resolver **fails closed** — it never
skips. `examples/workspaces/c/build-workspace-fixtures` is shared by **14
rows**, and 47 rows repo-wide use that same `build_subdir` name.

So an unattributable row is in tier 2's RUN set regardless of its coordinate,
while `lane=tier2` builds only the 14-coordinate cell cover. `workspace-c-native`
is `linux,c,zenoh`; tier 2's cover has `linux,c,cyclonedds`. The row is run and
never built.

## Evidence

After a core-crate edit (`nros-orchestration-ir`) staled every fixture:

```
just build-test-fixtures lane=tier2   → EXIT=0, stamp lane=tier2, 14 coordinates
just ci-matrix                        → _lane-gate PASSES
                                        test-all: 1706 run, 190 FAILED
```

Every failure is the same shape, in ~0.1 s:

```
Workspace fixture workspace-c-native is stale:
  examples/workspaces/c/build-workspace-fixtures/.nros-workspace-fixture.workspace-c-native.inputsig
Run `just native build-workspace-fixtures` first.
```

Spread across `native_api` (50), `native_example_reqresp_e2e` (28),
`workspace_features_e2e` (16), `rtos_e2e`, `threadx_riscv64_qemu`,
`zephyr_cortex_m_qemu` and more — including `ThreadxLinux::lang_1_Rust` and
`lang_3_Cpp`, whose coordinates are likewise outside the cover (tier 2 holds
only `threadx-linux,c,cyclonedds`).

The lane gate is not wrong about what it checks. It checks the cell cover, which
is fresh. The run then executes rows that are not in it.

## Why it stayed hidden

The same `just ci-matrix` was green an hour earlier on the same tree. The
difference was not the code — it was that a previous `lane=all` build had left
those artifacts fresh. **A tier-2 green is currently conditional on a broader
build having happened at some point in the past**, which is exactly the property
a lane is supposed to remove. A machine that has only ever run `lane=tier2` gets
190 failures; a machine with older `lane=all` residue gets green. Neither is
told which one it is.

This also makes tier 2 a trap after any core-crate edit: the CLAUDE.md
instruction is followed exactly, the lane gate passes, and the run fails on
freshness the lane never promised.

## Directions

1. **Make `nros_lane_build_lane` honest.** A lane's required build must be the
   union of its cell cover and every row its run set cannot skip. Unattributable
   rows are known statically — `row_artifact_root()` already computes the
   ambiguity — so the lane can include them.
2. **Or make the rows attributable** (issue 0517's direction): give each row its
   own `build_subdir`, so the resolver can skip out-of-lane rows and the cover
   becomes the true build set. 47 rows share one name today, so this is the
   larger change but the one that makes the invariant hold by construction.
3. **Failing either, the lane gate must check what the RUN will execute**, not
   what the cover contains — a gate narrower than the rule it enforces is the
   issue-0196 class, and this is a textbook instance.

Until then `just ci-matrix` after a core-crate change needs
`just build-test-fixtures lane=all`, and CLAUDE.md's tier-2 line overstates what
`lane=tier2` buys.

## Fixed — direction 1: the build asks the resolver's question

`--coords-from` now omits a row only when the RUN could have skipped it. The
ambiguity was already computed statically — `row_artifact_root`'s docstring
states the contract ("an ambiguous match is 'not attributable' — fail closed,
never skip") — so `row_is_lane_skippable()` reuses it rather than inventing a
second notion of attributability.

Measured on this tree:

```text
tier2 rows built — before: 114   after: 142   (lane=all: 258)
rows outside tier2's cover: 195, of those UNSKIPPABLE: 28
```

The 28 are exactly the added ones, and they cluster on shared roots such as
`examples/native/rust/talker/target` and
`packages/testing/nros-tests/bins/int32-sink/target`.

**Direction 2 (a unique `build_subdir` per row) is still the structural fix**
and is not done: it makes the invariant hold by construction rather than by a
predicate both sides must consult. This change makes the lane honest at the
cost of ~25 % more tier-2 rows; direction 2 would make it honest and cheap.
Left open as issue 0517's territory.

**The test checks MEMBERSHIP, and its first version did not.** It asserted
`built_rows >= unskippable` — 114 >= 28, which is true with the fix reverted,
so it passed the mutation it existed to catch. Now it asks the manifest which
unskippable rows are absent from the lane's build and requires the answer to be
none; reverting the fix fails it with 12 named rows for tier 1.

## Sweep

```sh
grep -c 'build_subdir = "build-workspace-fixtures"' examples/fixtures.toml   # 47
grep -n 'run_scope\|nros_lane_build_lane' -r packages/testing just
```
