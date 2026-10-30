---
id: 2111
title: "A ROS 2 Cyclone peer rejects the mps3-an536 FreeRTOS Cyclone talker's publication: PRESENTATION parameter invalid"
status: resolved
resolved: 2026-10-30
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

## Third sighting: tier 2 nightly (2026-10-27)

- Run: nightly 40466371508 (07:13 UTC, main at `5e863e1ee`).
- Job: 122062645403, `tier 2 nightly (pairwise cover)`.

The failing test was the same one as the second sighting, `freertos_qemu an536_cyclonedds_cpp_entry_delivers_within_the_dds_heap_default`. It panicked at `freertos_qemu.rs:157` on all three tries (TRY 3 at 120.3 s). It was one of the job's three real failures.

## The tier-2 sightings are a different defect, now fixed (2026-10-29)

Sightings 2 and 3 are not this issue. Both panicked at `freertos_qemu.rs:157`,
which is the `no nros: heap peak line` panic. The delivery assertion above it
passed. And `Received: 0` is the in-image listener's FIRST sample, whose counter
starts at 0; it does not mean that nothing arrived.

Reproduced 3 of 3 on a fixture rebuilt from `e0f81b4ac5` (2026-10-29): over 100
`Received:` lines per run, then the 120 s timeout. gdb on QEMU's gdbstub showed
the cause:

- The boot reporter `stkpeak` (`freertos_c_entry.c`) was created at priority 0,
  where it shares time slices with the idle task.
- The idle hook is a WFI. Under `-icount shift=auto` that left the reporter so
  few instructions per tick that one `uxTaskGetStackHighWaterMark` call, over
  13,461 free words, ran from tick 7180 to tick 36330: ~29 s of guest time per
  sample.
- The task was READY the whole time (priority-0 ready list intact, scheduler not
  suspended, time slicing compiled on), so nothing was hung. It was starved.

Fixed by creating it at priority 1, still below the app (3) and the transport
band (4), as the POSIX entry already did (`tskIDLE_PRIORITY + 1`). The Rust
entry's `heappeak` reporter had the same priority 0 and moved with it. After the
fix, the cell passes 3 of 3 in 28 s.

## Status of the PRESENTATION rejection

It is not reproduced since the first sighting. The next live-peer run, 40631028259
on main `e0f81b4ac5` (2026-10-28), passed
`nros_freertos_an536_cyclone_publisher_stays_reachable_from_ros2`. No commit in
`bca02e9ee..e0f81b4ac5` names it. It stays open until two more live-peer nights
are green, or it reappears and can be bisected.

## Resolution (2026-10-30): not reproduced, closed on the criterion above

The PRESENTATION rejection was seen once, in live-peer run 40448779789 on
`bca02e9ee` (2026-10-27). The same cell then passed on three consecutive
scheduled live-peer nights, each in the job that runs it ("rows whose board is
NOT this runner (ci image)"), and none of their logs carries a
`PRESENTATION` or `invalid parameter list` line from the host's Cyclone:

| run | main | job | cell |
| --- | --- | --- | --- |
| 40631028259 (2026-10-28) | `e0f81b4ac5` | — | PASS (recorded above) |
| 40816464981 (2026-10-29) | `3bb7c43cc4` | 123259119486 | PASS, 78.3 s |
| 41001586102 (2026-10-30) | `093456b7e2` | 123887524410 | PASS, 77.6 s |

That is the "two more live-peer nights green" this issue set as its closing
condition. It was never bisected, because it never reproduced: no commit in
`bca02e9ee..e0f81b4ac5` touches the QoS records, the Cyclone fork or the an536
board, and nano-ros never writes a PRESENTATION policy (the fork's
`ddsi_default_qos` supplies it, `git grep -i presentation -- packages/rmw/cyclonedds`
finds nothing). The two garbage booleans (`0x08`, `0x36`) therefore came from
memory the guest's Cyclone serialised, not from a setting of ours, and the
cause is unknown. If the rejection reappears, reopen this with the run id, and
bisect from the last green night with the single cell
(`pubsub_freertos_an536_cyclone_ros2_interop_e2e`), checking the fixture's mtime
against the blamed commit first.

The tier-2 sightings (2 and 3) were a different defect, fixed in
`a24c07ccf8` (the starved boot peak reporter) as recorded above.
