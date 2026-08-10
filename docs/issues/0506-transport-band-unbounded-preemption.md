---
id: 506
title: "Transport tasks above application tiers is the right default but has no budget — inbound overload preempts every tier for ~200 ms bursts"
status: open
type: enhancement
area: embedded
related: [issue-0505]
---

## Problem

Since the transport-priority fix (d708d8c5b), the recommended FreeRTOS
layout puts the transport band (tcpip_thread, zenoh read/lease, net
poll) above every application tier. That default is correct — the
inverse ordering starves the RX drain and produces multi-second lwIP
RTO freezes of the shared session, which is strictly worse. But the
transport band's CPU consumption is unbounded, so a remote peer that
publishes faster than the node can drain gets to preempt every
application tier for as long as the recovery takes.

Measured on the mps2-an385 FreeRTOS lane (QEMU icount, one TCP session
to zenohd, guest-clock timestamps; NEWSLabNTU/nano-ros-rt-eval
`docs/design.md` §8.1–8.2): sustainable drain is ~750 msg/s; under a
sustained ~1 kHz inbound flood the lane enters periodic recovery
cycles — inbound delivery collapses from ~780/s to 15–260/s for 1–2 s,
and during each cycle the transport band runs solid for ~100–340 ms of
guest time, stalling all tiers simultaneously (every periodic task's
timestamps gap at the same instant, then the timer backlog replays,
issue 0505). The application's declared periods are 10–100 ms; a
single misbehaving or compromised publisher on a subscribed topic can
therefore blow every deadline on the device at will, and no local
scheduling configuration can prevent it — the tiers are below the band
by design.

## Fix direction

Bound the ingress work, not the priority:

- **Budgeted drain (preferred)**: run the RX/drain path as a
  sporadic-server-style budget — N ms of transport execution per
  replenishment period, overflow deferred. Overload then degrades the
  flooded topic (queue tail-drop at the rmw layer) instead of
  degrading every tier's timing. FreeRTOS has no native sporadic
  server, so this is a self-suspending drain loop with a budget check,
  which the zenoh read/poll tasks already have the structure for.
- **Cheaper partial**: per-subscription inbound rate cap / ring-depth
  drop policy at the rmw boundary, so a flood costs one bounded queue
  rather than unbounded protocol recovery (today the overrun cascades
  into TCP backpressure and the recovery burst is what preempts the
  tiers).
- Either way, expose a drop/deferral counter — silent shedding is how
  this class of problem hides (same observability argument as issue
  0505).

Related direction: if launch-level scheduling metadata grows an
"ingress budget" dimension alongside rates and deadlines, the per-kernel
realization above stops being a hand-tuned constant. That is a design
discussion (RFC-sized), not part of this issue's minimal fix.
