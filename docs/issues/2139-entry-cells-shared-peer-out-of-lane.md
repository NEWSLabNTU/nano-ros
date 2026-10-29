---
id: 2139
title: "Seven `entry_e2e` cells skip on every tier-2 nightly, because their shared observer's own coordinate is out of lane"
status: open
type: bug
area: [testing, ci]
severity: medium
found: 2026-10-29
related: [2010, 2004, 0828, phase-513]
---

## Measured

- Run: nightly 40648207713 (schedule, 2026-10-28 07:08 UTC, main at
  `e0f81b4ac5`).
- Job: `tier 2 nightly (pairwise cover)`, artifact `tier2-nightly-junit`.
- Test: `nros-tests::entry_e2e entry_matrix`, which PASSED with
  `entry_matrix: 4 ran, 13 skipped, 0 failed (of 17 cells)`.

Seven of the 13 skips name the same fixture, and it is not the cell's own:

```
zephyr/c/entry_pubsub:          [SKIPPED:lane] out of lane: workspace-c-native-robot2 is at coordinate linux,c,zenoh, ...
zephyr/mixed/entry_pubsub:      [SKIPPED:lane] out of lane: workspace-c-native-robot2 ...
freertos/c/entry_pubsub:        [SKIPPED:lane] out of lane: workspace-c-native-robot2 ...
freertos/mixed/entry_pubsub:    [SKIPPED:lane] out of lane: workspace-c-native-robot2 ...
nuttx-arm/c/entry_pubsub:       [SKIPPED:lane] out of lane: workspace-c-native-robot2 ...
threadx-linux/mixed/entry_pubsub: [SKIPPED:lane] out of lane: workspace-c-native-robot2 ...
threadx-riscv64/c/entry_pubsub: [SKIPPED:lane] out of lane: workspace-c-native-robot2 ...
```

## Reading

`entry_e2e`'s `EntryPubsub` cells check delivery with one shared observer,
the C `linux_robot2_entry` listener (`spawn_c_listener_observer`, resolved by
`build_native_workspace_c_entry_robot2`). That fixture's row,
`workspace-c-native-robot2`, sits at `linux,c,zenoh`. The tier-2 nightly's
pairwise cover does not select that coordinate, so the resolver reports the
observer out of lane, and the cell skips. It does not matter whether the
cell's own coordinate (`zephyr,mixed,zenoh`, ...) is in the lane.

So the coordinate model has no way to say "this cell also needs that row".
A lane build is one predicate over each row's own coordinate (`row_coord()`),
and the run side asks the same predicate of every artifact it resolves. A
peer fixture shared by cells on OTHER coordinates is therefore built and run
only when its own coordinate is in the lane.

Consequence: phase-513 R1's acceptance ("a tier-2 nightly where
`build-ws-mixed-entry-zenoh` builds, and `entry_e2e`'s zephyr mixed cell
runs") cannot be met by any tier-2 nightly as configured. The cell is skipped
before its own fixture is asked about.

The skip is not wrong as a skip: the lane really did not build the observer.
The gap is that seven cells' only verdict is "out of lane" on every night,
and `entry_matrix` reports PASS over them.

## What it is NOT

- Not a fixture build failure. Nothing was built and failed.
- Not issue 2004 (a matrix test that passes over cells it did not run prints
  which ones), though it is why this was visible at all.

## What would close it

One of:

1. Let a cell declare the peer rows it needs, so the lane BUILD includes a
   peer whenever a consuming cell is in the lane, and the run side resolves
   it as in-lane for that cell. This keeps build-set and run-set one
   predicate, now over the cell's coordinate plus its declared peers.
2. Or give the observer a row at every coordinate that uses it. That is
   seven copies of one binary, so (1) is preferred.

Acceptance: a tier-2 nightly in which the seven cells above RUN.
