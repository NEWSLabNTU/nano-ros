---
id: 1929
title: "zenoh's async arrival wake is ONE process-global slot, so with two zenoh sessions in a process the first session's arrivals wake the second's executor"
status: open
type: bug
severity: medium
area: [rmw, executor]
related: [issue-0317, phase-436, phase-397]
found: 2026-10-16
---

## Measured

Phase-436 step 5's fan-out probe, native Linux (aarch64), zenoh, one process:

- An owner executor opened a session and registered a subscription.
- A publisher executor in the SAME process opened a second session
  (`NROS_ZENOH_MAX_SESSIONS=2`) and published at 1 kHz.
- The owner, in `spin_once(1000 ms)`, spun **once per second** and received
  **5 of 4999** samples in 5 s.

With the publisher moved to its own process, the same owner spun 999 times a
second and received 4997 of 5000. Nothing else changed.

## Cause

`Session::set_wake_callback` (`nros-rmw-zenoh/src/shim/session.rs`) stores the
callback per session, and ALSO mirrors it into a process-global,
`RUNTIME_WAKE_CB` / `RUNTIME_WAKE_CTX` (`shim/mod.rs`, issue 0317). The global
is what the real arrival hook fires: `subscriber_notify_callback` and its
service twins call `fire_runtime_wake()`, on the zenoh-pico read task. The
per-session copy is fired only from `drive_io`'s poll path, which on the
multi-threaded backend sees no work.

So the LAST session to install a wake callback owns every arrival in the
process. The first session's executor is never woken by its own data; it waits
out its bound and then takes one sample per spin. The comment beside the global
states the assumption it rests on: "One executor/session per embedded image,
so a single global slot is sufficient."

## Who is affected

A process that opens two zenoh sessions under DIFFERENT executors: a test or
host app with two `Executor::open`s, and any image that raises
`NROS_ZENOH_MAX_SESSIONS` to do that. Not affected:

- several executors over ONE session (spawned tiers, C executors over one
  support context), since they share one fan-out context (phase-436);
- an extra session on the SAME executor (`node_builder.rmw(...)`), which
  installs the same executor context.

## Fix direction

Fire the wake through the session the arrival belongs to. The arrival hook
already holds its subscriber's buffer, so it can reach the owning session's
`wake_cb`/`wake_ctx` instead of the global. RFC-0084's entity-scoped wake
registration (phase-397) would make this structural: the arrival names the
entity, and the entity names its executor.
