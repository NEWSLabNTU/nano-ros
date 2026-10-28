---
id: 2125
title: "Two rtos_e2e cells that run one baked image start routers on one port, and the second start kills the first one's router"
status: open
type: bug
area: [testing, ci]
severity: medium
found: 2026-10-29
related: [phase-466, 2094, 2126, 0470]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, since 2026-10-29.

## Measured

Both tier-2 nightlies on 2026-10-28 (runs 40636297544 and 40648207713, runner
118) failed the same two cells:

- `test_rtos_quiet_link_keeps_its_session::platform_1_Platform__Freertos`
  printed `WARNING: port 7800 already in use — killing orphaned process`, then
  `port 7800 still in use after kill attempt`. The listener could not declare
  its node (`z_liveliness_declare_token failed: -103`,
  `NodeError::Transport(ConnectionFailed)`), so its readiness line
  `Application setup complete` never appeared.
- `test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust`
  delivered 70 of 70 messages, then failed its session-churn check: the router
  log `zenohd-9000.log` held one `New transport opened` line where two nodes
  had connected. That is [issue 2126](2126-threadx-linux-pubsub-router-log-one-transport.md).

## Cause

The quiet-link cell (added 2026-10-27 for issue 2094) runs the pub/sub Rust
LISTENER image. That image bakes its router's port at build time (7800 on
FreeRTOS, 9000 on threadx-linux), so the cell starts its router on the same
port as `test_rtos_pubsub_e2e`'s Rust cell for that platform. Both are in one
nextest group per platform, and the FreeRTOS group runs 9 at a time.

`ZenohRouter::start_on_with` treated any listener on its port as an orphan
from an earlier run and killed it with `fuser -k`. A live router belonging to a
concurrent test is not an orphan, so whichever of the two cells started second
killed the other's router mid-run. It also truncated the shared per-port log
file, `zenohd-<port>.log`, that both cells read their session count from.

NuttX did not fail, because its group runs one test at a time.

## Fix

A router start on a NAMED port now waits for that port instead of taking it.
`port_lease::wait_for_port` takes the same per-port `flock` that
`lease_port` uses for ephemeral ports, and waits up to 240 s
(`NAMED_PORT_WAIT`) for a live holder to let go. Only after it holds the lock
does `kill_listeners_on_port` run, so what it kills really is an orphan: a
dead process holds no `flock`. A holder past the budget fails the start and
names its pid. A port this process already holds is not waited for (a
`start_unique` that leased the port and then starts on it).

This fixes the class, not the two cells: any two tests that run images baked
with one router port now take turns, whichever groups they are in.

Unit tests: `a_named_port_waits_for_its_holder_and_is_handed_over` and
`a_named_port_held_past_the_budget_fails_naming_the_holder`.

## What would close it

A tier-2 nightly where both cells pass. Their logs should show `port <n>: held
by pid … — waiting` when they overlap, and no `killing orphaned process`.
