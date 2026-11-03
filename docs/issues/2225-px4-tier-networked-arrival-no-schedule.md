---
id: 2225
title: "A PX4 tier hears a networked backend's arrival only at its next deadline"
status: open
type: limitation
severity: medium
area: [px4, rmw, executor]
related: [phase-521, 2218]
found: 2026-11-03
---

## What

A PX4 tier is a work item (RFC-0113 D9, phase-521 W8): it waits on its
schedule, never on a parked thread. uORB arrivals schedule the owning tier
directly. Each subscription that a tier's setup creates calls the tier's
`TierWorkItem::notify()` from the publisher's context
(`nros_rmw_uorb_set_arrival_target`, issue 2218).

A networked backend linked into the same module (the bridge's outward zenoh
session) reports arrivals through the executor's wake callback. That signals
the executor's platform wake object, which no PX4 tier waits on. A
subscription on such a session is therefore served at the tier's next
deadline (a timer, or the session's own next internal event), not on
arrival. That breaks D2.3 for that subscription.

## Why it is not fixed here

The in-tree PX4 images subscribe only to uORB. The bridge's networked side
only publishes. Fixing this needs the executor's wake to reach a framework's
scheduler: the wake object would have to signal through a slot the framework
fills, as the park slot does. That is an executor ABI change, which RFC-0113
D7 asks to rule on rather than add in passing.

## Workaround

Put a networked subscription on a tier with a timer whose period bounds the
latency you can accept.
