---
id: 2217
title: "A Cyclone build without foreign wake (freertos-posix) still reads as `Notify`, so its executor parks on a wake nothing posts"
status: open
type: bug
severity: medium
area: [rmw, cyclonedds, executor]
related: [phase-521, 1237]
found: 2026-11-02
---

## What

`NROS_RMW_CYCLONEDDS_FOREIGN_WAKE=OFF` (set by the freertos-posix board, issue
1237) makes `session_set_wake_callback` answer `UNSUPPORTED`, because Cyclone's
receive thread is not a FreeRTOS task there. But the Cyclone vtable fills the
`set_wake_callback` slot and no `readiness_source` slot, and
`CffiSession::readiness_source` infers `Notify` from the slot's presence.
`install_wake_signal_on_primary` likewise marks the wake fan-out live from
`supports_wake_callback` (slot presence). So the session reads as `Notify`
with a live callback that never fires.

Found by reading while landing phase-521 W6 (RFC-0113 D5's refusal at open),
not measured. Consequence by reading: `entry_spin_budget` gives such an
executor the 60 s idle budget, and an arrival waits for the next timer or
session deadline instead of ending the park.

## Fix direction

Give the Cyclone vtable a `readiness_source` slot that answers `Notify` only
when the listener is installed. On a NO_FOREIGN_WAKE build it then states
`None` and is refused at open (RFC-0113 D5), unless that board gets a form
first: a port-owned thread that turns Cyclone's arrival into a wake (the
threadx-linux NSOS watcher in phase-521 W6f is the same shape), or a Waitable
handle.
