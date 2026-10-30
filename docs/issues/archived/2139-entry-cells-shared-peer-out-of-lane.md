---
id: 2139
title: "Seven `entry_e2e` cells skip on every tier-2 nightly, because their shared observer's own coordinate is out of lane"
status: resolved
type: bug
area: [testing, ci]
severity: medium
found: 2026-10-29
resolved: 2026-10-30
resolved_in: \"nightly 41018247377 (phase-513)\"
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

## Fixed in the tree, 2026-10-29 (still open)

Option 1, in the shape the manifest already had: the observer's row declares
the coordinates it serves, and the lane predicate reads them on both sides.

- `[[workspace_fixture]]` rows take `serves = ["platform,lang,rmw", ...]`.
  `workspace-c-native-robot2` lists the eleven `entry_e2e` cells that spawn it:
  ThreadX-Linux c/cpp/mixed, FreeRTOS c/cpp/mixed, Zephyr c/cpp/mixed, NuttX c
  and ThreadX riscv64 c. That is more than the seven of the issue's night. The
  other four skipped that night for their own reasons, and they use the same
  observer.
- **Build side.** `fixtures-manifest.py` has `row_in_coords()`: a row is
  selected when its own coordinate or one it serves is in the lane.
  `--coords-from` and the west-leaf filter both go through it.
- **Run side.** The `coords` record gains a ninth field, `serves`, and
  `lane::is_in_lane` makes the same test. The three parsers that pinned the
  record at eight fields now pin nine.
- **Scheduling.** `lane-coords --modules` adds the module of a peer row that is
  in the lane only through `serves`. Without that, a lane with those cells and
  no Linux cell would build no module that produces the observer, and then fail
  to resolve it. The tier-2 nightly has Linux cells, so this is not what bit
  it.
- **Gate.** `entry_e2e::c_listener_observer_serves_every_cell_that_uses_it`
  compares the row's `serves` with the cells whose proof is
  `ChatterToCListener`, in both directions, and names each missing or extra
  coordinate. It needs no fixture. Mutation-tested: removing
  `"zephyr,mixed,zenoh"` fails it naming that coordinate. It is not in
  `.config/lane-admission/gate.txt` yet. That file is generated by the lane
  census, so the test joins the gate lane at the next regeneration. Until then
  it runs wherever `entry_e2e` does.

Measured on 2026-10-29. `fixtures-manifest.py list-workspaces --platform linux
--coords-from` over the single line `zephyr,mixed,zenoh` now selects
`workspace-c-native-robot2`. `lane_run_narrowing`, `lane_build_covers_run` and
`matrix_fixture_coverage` all pass.

**Acceptance is unchanged:** a tier-2 nightly in which those cells RUN.

## Resolved, 2026-10-30

Accepted on nightly 41018247377 (schedule, 2026-10-30 07:08 UTC, main at
`1c81f56b75`), job 123939362475 `tier 2 nightly (pairwise cover)`, artifact
`tier2-nightly-junit`. `entry_e2e entry_matrix` reported
`entry_matrix: 11 ran, 6 skipped, 0 failed (of 17 cells)`, against 4 ran on
the night this was filed. None of the 6 skips names
`workspace-c-native-robot2`: each names the cell's OWN fixture at a
coordinate the pairwise cover does not select (`zephyr,cpp`, `freertos,cpp`,
`threadx-linux,c`, `threadx-linux,cpp`), which is a correct lane skip. All
seven cells listed under "Measured" ran and passed.

The gate test joined `.config/lane-admission/gate.txt` in #2856, after
`check-lane-contracts` stopped reading a fn signature as a call.
