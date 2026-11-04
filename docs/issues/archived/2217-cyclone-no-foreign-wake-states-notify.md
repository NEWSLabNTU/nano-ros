---
id: 2217
title: "A Cyclone build without foreign wake (freertos-posix) still reads as `Notify`, so its executor parks on a wake nothing posts"
status: resolved
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

## Resolution

Fixed 2026-11-04. The Cyclone vtable fills `readiness_source`: `Notify` when
the listener is installed, `None` otherwise, so a build that cannot post a
wake is refused at open (RFC-0113 D5) instead of parking on a wake nothing
signals.

The two boards that build without foreign wake got a form rather than a
refusal. A new platform seam, `nros/platform_foreign_wake.h` (bind / unbind
/ post), lets a foreign thread request a call in kernel context: post stores
a flag and writes a host eventfd, entering no kernel service. On
freertos-posix a lowest-priority FreeRTOS task blocks in `poll()` on that
eventfd; on threadx-linux the eventfd joins the existing NSOS receive
watcher (phase-521 W6f). Cyclone's `on_data_available` posts the session's
slot, and the forwarder calls the executor's wake callback. The boards turn
it on with `NROS_RMW_CYCLONEDDS_FOREIGN_WAKE_BRIDGE`.

Evidence: threadx-linux `test_threadx_linux_cyclonedds_*` 4/4 and
freertos-posix `freertos_posix_{c,cpp}_entry_delivers_over_cyclonedds` pass.
