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

## Discussion agenda (2026-08-11, pre-RFC)

What already exists, and why it does not cover this: the subscriber
ring has a depth knob (`ZPICO_SUBSCRIBER_RING_DEPTH`, default 4) and an
`overflow_drops_total` counter, and the rmw carries ROS QoS reliability
(RELIABLE/BEST_EFFORT) in its keyexpr encoding. Both sit ABOVE the
transport: a ring drop happens after the TCP/session layer already
spent the CPU to receive, reassemble, and route the message. The
observed ~100-340 ms preemption bursts are that lower layer recovering
— so message-level shedding, however configured, cannot bound them.

1. **Which layer takes the budget.** Candidates, cheapest first:
   (a) *TCP window shaping* — shrink the lwIP receive window
   (`TCP_WND` / per-pcb rcv buf) so a too-fast sender is throttled by
   flow control continuously instead of by RTO storms episodically.
   Config-only, no scheduler work, and testable today on the QEMU lane.
   Open question whether a persistently small window produces smooth
   pacing or just moves the oscillation.
   (b) *Budgeted drain loop* — the zenoh read/lease/poll tasks
   self-suspend after N ms (or N messages) per replenishment period; a
   sporadic server in application clothing. FreeRTOS has no native
   execution-time budget, so this is cooperative, but the drain loops
   are already structured as bounded iterations. Interaction to
   analyze: deferring drain FILLS the TCP window faster — which is
   (a) by another route — but an under-provisioned budget recreates
   the RTO freeze the transport-priority fix eliminated. The two
   mechanisms likely want to be tuned together or (a) alone may
   suffice.
   (c) *Kernel-level budgets* — nothing portable: FreeRTOS has none,
   Zephyr has no CBS either. Rejecting (c) should be on the record.
2. **Does QoS already spell part of this?** If BEST_EFFORT on the
   flooded topic mapped to a zenoh channel that drops at the
   TRANSPORT (sender- or link-side) rather than at the ring, the
   existing QoS surface would express the intent for cooperative
   publishers. Needs a factual check of what zenoh-pico does with
   reliability on a client session. It cannot be the whole answer —
   the threat model includes a non-cooperating/compromised publisher,
   and only receiver-side bounding helps there — but the RFC should
   position the new dim relative to QoS, not beside it.
3. **Session topology.** Chain-critical topics and the flood share one
   TCP session, so transport recovery is head-of-line blocking for
   everything. Per-criticality sessions (or zenoh priority channels,
   if zenoh-pico exposes them) would isolate the blast radius at the
   cost of connection state. Decide whether topology is in scope for
   the RFC or an explicit non-goal.
4. **Contract shape.** If launch-level scheduling metadata grows an
   ingress dimension: per what? Per-subscription msg/s is what a user
   can state; per-session drain ms/period is what the mechanism
   enforces; the resolver would map one to the other against the
   measured drain envelope (the ~750 msg/s number is lane-specific, so
   the mapping needs a per-platform capacity input, not a constant).
   Also decide the failure mode: reject at resolve time when declared
   ingress exceeds capacity, or admit and shed with a counter.
5. **Observability.** Whatever sheds must count: shed/deferral
   counters at the drain loop and per-subscription, surfaced like
   `overflow_drops_total`, plus the #505 overrun counter as the
   tier-side symptom of the same pressure. Silent shedding is how this
   class of problem hides.
6. **Evidence to gather before writing the RFC** (all runnable on the
   existing QEMU lane): (i) a Tonbandgeraet trace during a flood
   recovery burst to attribute the ~200 ms between tcpip_thread /
   zpico_read / lease — the budget should target the actual consumer;
   (ii) the TCP_WND-shaping experiment from (1a) under the same 2x500
   Hz flood, measuring both tier gaps and chain delivery; (iii) ring
   depth 1 vs 4 to confirm ring settings genuinely do not move the
   burst (closes the "just tune the ring" objection).
7. **Security framing.** A subscribed topic is an unauthenticated
   remote input that can consume unbounded CPU above every tier:
   ingress budgeting is a DoS-surface reduction, not only a
   scheduling nicety. Worth a sentence in the RFC's motivation.

## Experiment results (2026-08-11)

Ran the agenda's items (ii) and (iii) on the QEMU mps2-an385 lane
(3-phase demo, guest-clock `t_us` cadence, 2 runs per cell unless
noted). A 1 kHz flood turned out to sit at the edge — some sessions
produce no stalls at all — so the discriminating cells are at 2 kHz.
Full table: `results/issue506_experiments.md` in the evaluation
workspace (`tools/analyze_506.py`).

| cell (2 kHz flood) | stalls/run | worst gap | miss % | rx/s | chain % | chain p50 |
|---|---|---|---|---|---|---|
| default (WND 4xMSS, ring 4) | 12.0 | 403 ms | 1.71 | 320 | 12.1 | 45 ms |
| subscriber ring depth 1 | 10.0 | 511 ms | 1.12 | 23 | 4.6 | 237 ms |
| TCP_WND 1xMSS | **0.0** | **12 ms** | **0.00** | 10 | 14.8 | **446 ms** |
| TCP_WND 8xMSS | 151.0 | 110 ms | 23.14 | 0 | 0.0 | no delivery |

Three findings, one of which changes the design direction:

1. **The subscriber ring is not the lever** (closes the "just tune the
   ring" objection). Depth 1 vs 4 leaves the stalls intact (10 vs 12
   per run, worst gap no better) while making delivery strictly worse
   — the drain collapses to 23 msg/s and chain delivery falls from
   12.1% to 4.6%. Message-level shedding happens after the transport
   has already spent the CPU, exactly as the problem statement
   predicted.

2. **The receive window IS the lever — in both directions.** 8xMSS is
   catastrophic (151 stalls/run, 23% of ctrl periods missed, inbound
   drain and chain both to zero): a bigger window admits more flood
   for the transport band to process, and the island does nothing else.
   1xMSS eliminates the stalls entirely (0 per run, worst gap 12 ms,
   0.00% missed). This is direct evidence that the tiers' timing is
   governed by how much inbound work the transport is allowed to
   accept — i.e. that an ingress budget is the right shape of fix.

3. **But flow control alone does NOT solve it, and this is the
   important one.** Under 1xMSS the cadence is perfect while chain
   command latency goes to **446 ms p50** (vs 45 ms at default, 17 ms
   unloaded). A shrunken window does not shed the flood; it queues it,
   and because the flood and the safety-critical chain share ONE
   reliable, ordered TCP stream, the chain is head-of-line blocked
   behind flood bytes. Trading a 400 ms scheduling stall for a 450 ms
   stale command is not a fix for a control path.

Consequences for the RFC:

- Agenda item (1a) "TCP window shaping" is demoted from cheapest
  candidate to **not viable alone**. It is still the clearest
  demonstration of the mechanism, and may be a useful safety valve for
  a device whose subscriptions are all best-effort telemetry.
- Agenda item (3) "session topology" is promoted from optional to
  **necessary**: any ingress bound that acts on a shared reliable
  stream converts CPU preemption into head-of-line delay on the
  critical path. Separating criticality classes onto distinct
  sessions/streams (or a lossy channel for flood-class topics) is a
  precondition for a budget that helps rather than relocates the harm.
- The budgeted-drain option (1b) inherits the same caveat: deferring
  drain fills the window, which is the 1xMSS experiment by another
  route. It is only safe once the critical path cannot be blocked by
  flood-class traffic.
- Practical interim guidance for integrators, worth documenting
  independently of the RFC: do NOT raise `TCP_WND` on a device with
  real-time tiers. The 8xMSS cell shows the failure is not gradual.

Item (i) of the agenda (a Tonbandgeraet trace during a recovery burst,
to attribute the ~200 ms between tcpip_thread / zpico_read / lease) is
still open; the trace lane does not currently run with a load
generator.
