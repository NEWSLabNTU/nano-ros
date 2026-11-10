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

- The row's image bakes its router locator (`baked_row_locator`), so the test
  starts a router on that baked port, 9100, rather than on a leased one.
- Another test held port 9100 when this one started, and the test waited for
  it. The other 155 entries registered.
- The entry runs under `BAKED_RUN_BUDGET` (10 s) and is judged on its setup
  line. A session that takes longer than that to open after the router comes
  up would read as "refused".

## Not known

Whether the entry dialed before the new router listened, met the previous
holder's router, or simply ran out of its 10 s budget under load. The
entry's own output is not in the panic. A recurrence should be read for it
before changing the budget or the port handling.
