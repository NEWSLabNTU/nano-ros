---
id: 2420
title: "`rmw_coordinate_truth`: the freertos-posix zenoh C entry once refused its session after waiting for its baked router port"
status: open
type: bug
severity: low
area: [tests, rmw-zenoh, freertos]
related: [2416, 1819, 1933]
found: 2026-11-10
---

## Measured

Scheduled tier-2 run 42767563994, job 129678696882 "tier 1 (cells)":
`rmw_coordinate_truth a_rows_entry_registers_its_nodes_at_runtime` failed
in 48.6 s. Its junit (artifact `tier1-junit`) holds the panic:

```
port 9100: held by pid 1428289 (another test on the same baked port) — waiting up to 240s
rmw-coordinate-truth: ran 156 entries in 23.6s on a pool of 8 (49 cross-target row(s) left to their QEMU cells); ...
thread 'a_rows_entry_registers_its_nodes_at_runtime' panicked at packages/testing/nros-tests/tests/rmw_coordinate_truth.rs:986:5:
1 generated entry did not register with the peer this test started for it (issue 1819 — a missing peer is no longer an excuse):
  workspace-c-freertos-posix-zenoh (zenoh) — the backend refused the session although its peer was started for it
```

The same test passed in 37.1 s in run 42744194409 four hours earlier, so this
is intermittent, not a regression on main.

## What is known

- The failing row's image bakes its router locator (`baked_row_locator`):
  `examples/workspaces/c/src/demo_bringup/system.toml` `[image.freertos_posix_zenoh]`
  states `tcp/127.0.0.1:11130`. The test starts a router on that port. Nothing
  else in the tree names 11130.
- The `port 9100: held by …` line in the same panic is about a DIFFERENT row:
  9100 is the threadx-linux C pubsub port (`platform.rs` `THREADX_LINUX`), baked
  by `examples/workspaces/c/src/pubsub_bringup` and the threadx-linux C talker
  fixture row. The junit timestamps name the holder:
  `rtos_e2e test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_2_Lang__C`
  ran 07:27:40.7 to 07:28:53.3, and the coordinate-truth test ran 07:28:30.4 to
  07:29:19.0. No other test longer than 1 s overlapped.
- So two tests in one lane share a baked router port. The coordinate-truth test
  waits for it rather than colliding, which is why that row registered, but the
  wait is an isolation gap, not noise: it serialises the two tests on a port
  neither chose, and either one can meet the other's router if the wait ever
  misjudges the holder.
- The other 155 entries, the 9100 row among them, registered. The failing row
  runs under `BAKED_RUN_BUDGET` (10 s) and is judged on its setup line. A
  session that takes longer than that to open, for instance under the load of
  the 9100 wait and a pool of 8, would read as "refused".

## Not known

Whether the 11130 entry dialed before its router listened or simply ran out
of its 10 s budget under load. Its own output is not in the panic; a
recurrence should be read for it before changing the budget. Separately, the
9100 sharing between `rtos_e2e` and this test is a candidate for its own fix
(one owner per baked port per lane).
