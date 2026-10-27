---
id: 2111
title: "A ROS 2 Cyclone peer rejects the mps3-an536 FreeRTOS Cyclone talker's publication: PRESENTATION parameter invalid"
status: open
type: bug
area: [cyclonedds, freertos, interop, qos]
severity: medium
found: 2026-10-27
related: [0917, 1981]
---

## Measured

`live-peer regression` run 40448779789 on main `bca02e9ee` (scheduled,
2026-10-27 04:15 UTC), job 122006758906 "rows whose board is NOT this runner
(ci image)", step `test-live-peer-regression`:

```
FAIL [  78.010s] nros-tests::pubsub_freertos_an536_cyclone_ros2_interop_e2e
     nros_freertos_an536_cyclone_publisher_stays_reachable_from_ros2
`ros2 topic echo` (rmw_cyclonedds_cpp, domain 54, tag nros-441-w6-an536) received
nothing from the mps3-an536 FreeRTOS Cyclone talker in 75s over slirp (phase-441 W6).
echo output:
  dq.builtin: invalid parameter list (vendor 1.16, version 2.1): pid 21 (PRESENTATION) invalid, input = 0,0,0,0,8,54,0,0
  dq.builtin: Invalid PublicationBuiltinTopicData (vendor 1.16): invalid qos/parameters
--- guest console ---
  Published: 0
  [INFO] nros: cpp_talker logging seq=0
```

The guest boots and its C++ talker runs; the host's Cyclone receives the
talker's SEDP publication announcement and **rejects it** because the
PRESENTATION QoS parameter (pid 0x0015) is malformed. The decoded bytes read as
access scope `0` followed by `coherent_access = 0x08` and `ordered_access =
0x36`: the two booleans hold non-0/1 values, which Cyclone's
`dd_qos` validation refuses. That looks like uninitialised or misaligned
memory reaching the wire, not a deliberate setting.

## It is new

The same job passed on the three previous nights (runs 40268381374 on
`60c7c7388`, 40161216248, 40061591574). Those runs were red only in the
**board** job ("rows whose board IS this runner"), and once on
`xrce_ros2_interop test_ros2_service_xrce_client`, so this cell was green until
somewhere in `60c7c7388..bca02e9ee` (102 commits).

## What it is not

- Not the disk class (issue 1353): the run printed a verdict, and the log has no
  `No space left`.
- Not issue 0917 (an536 fragmented samples never syncing): here discovery itself
  is refused, so no sample is ever matched.
- Probably not a peer-side change: the rejection names the guest's bytes.
  Whether the CI image's `rmw_cyclonedds_cpp` moved between the green and red
  nights was NOT checked.

## Where to look

Commits in the range that touch QoS records or the C/C++ entity-creation path
the C++ talker uses: `a89e2e69c2` (QoS duration builders), `0144996c1f`
(`PrimitiveOptions` at create sites), `22043aaf3a` (`IntoNodeOptions`),
`47c665ba30` (executor FFI behind a sealed trait), `0ed2a03430` and
`cba87b6547` (C ABI changes). A QoS struct that gained or moved a field while a
hand-mirrored or `repr(C)` copy did not would produce exactly garbage booleans
(CLAUDE.md: hand-mirrored FFI structs drift on append).

## What would close it

Bisect `60c7c7388..bca02e9ee` with the single cell
(`pubsub_freertos_an536_cyclone_ros2_interop_e2e`), fix the commit that
corrupts the PRESENTATION parameter, and show the cell green in a live-peer run.
If it does not reproduce on a rebuilt fixture, record that the fixture was
stale (check the artifact's mtime against the blamed commit first).

## Second sighting: tier 2, the an536 Cyclone C++ entry (2026-10-27)

`run-matrix` run 40461297569 (main `99d5e1df0`), job 122046305909, one of its
two real failures:

```
nros-tests::freertos_qemu an536_cyclonedds_cpp_entry_delivers_within_the_dds_heap_default
panicked at packages/testing/nros-tests/tests/freertos_qemu.rs:157:9:
no `nros: heap peak` line within 120 s.
  ... mps3-an536 ... -kernel .../freertos-cyclonedds-mps3-an536-freertos-.../mps3_an536_entry
  Network ready
  Published: 0
  [INFO] nros: cpp_talker logging seq=0
  Received: 0
```

The same board and the same Cyclone C++ talker, this time paired with our own
listener: it publishes, and nothing is received. That fits a broken SEDP
announcement, since our Cyclone listener would refuse the same malformed
PRESENTATION as the ROS 2 peer did. It is unconfirmed until the bisect. This was
the first tier-2 run in days to reach its cells (issue 2025 stopped the earlier
ones), so it says nothing about when this began.
