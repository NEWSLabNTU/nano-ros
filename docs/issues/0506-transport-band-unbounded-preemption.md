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

## Attribution (2026-08-11) — agenda item (i), closed

The trace lane previously ran idle only, which is why "which task burns
the ~200 ms" stayed open. It now takes a load level, so a Tonbandgeraet
capture can run under the flood that produces the stalls. Per-task
occupancy over the captured window (FreeRTOS mps2-an385, ~2 kHz flood,
two captures that landed on a burst; idle capture for contrast):

| task | idle | under ~2 kHz flood |
|---|---|---|
| `zpico_read` (zenoh-pico session read loop) | 0-1.2% | **72-81%** |
| `net_poll` | 0.1-0.2% | 3.2% |
| `tcpip_thread` (lwIP) | 0.4-0.5% | 1.8% |
| application tiers (all three) | 4.1-4.4% | **2.3-3.0%** |
| IDLE | 94-95% | 11-20% |

**The band that preempts the tiers is the zenoh-pico read loop, not the
TCP/IP stack.** lwIP's own thread stays under 2% even while the island
is being starved. So a budget belongs on the rmw drain — the loop that
takes messages off the session and dispatches them — and NOT on
`tcpip_thread`, which was the other obvious suspect.

That also re-reads the TCP_WND experiment above: shrinking the receive
window worked not by making lwIP cheaper but by starving `zpico_read`
of work, which is why it fixed cadence and wrecked chain latency at the
same time. Bounding the drain directly is the version of that lever
which can distinguish flood traffic from the safety chain.

Reading note for anyone repeating this: the trace buffer holds a fixed
number of EVENTS, so a busy window fills it in ~35 ms while a quiet one
spans ~180 ms. A short span is itself the signal that the capture landed
on a recovery burst; two loaded captures that happened to land in a
quiet moment show the same profile as idle.

Tooling: `just freertos-trace <load>` and `tools/analyze_trace_cpu.py`
in the evaluation workspace; raw numbers in
`results/issue506_trace_cpu.md`.

### What remains before the RFC

1. **Unchecked fact:** what zenoh-pico does with BEST_EFFORT on a client
   session — whether the existing QoS surface can already express
   "shed this topic at the transport" for cooperative publishers.
2. **The budget's shape:** with the consumer identified, the sporadic
   -server-style drain budget (agenda 1b) now has a concrete home. It
   still inherits the head-of-line caveat: bounding the drain on a
   shared reliable stream converts CPU preemption into queueing delay
   on the critical path, so it needs the session/stream separation
   (agenda 3) to help rather than relocate the harm.
3. **Where the budget is declared:** per-subscription msg/s is what a
   user can state, drain-us per period is what the mechanism enforces,
   and the mapping needs a per-platform capacity input (the ~750 msg/s
   figure is lane-specific).

## Design exploration (2026-08-11)

With the consumer identified (`zpico_read`, 72-81% of CPU under flood),
the question is which layer can bound it. Five mechanisms exist or could
be built; they are NOT alternatives, they act at different points and
only one of them actually reduces the island's work.

### What each mechanism can and cannot do

**1. Router-side rate limiting (`downsampling`, `low_pass_filter`).**
zenohd's config carries both (visible in its startup dump). These drop
messages AT THE ROUTER, so the flood never reaches the MCU's link and
never costs it a decode. This is the only mechanism in the list that
reduces the island's per-message work without the island doing the
work first. Cost: it lives in the router's config, i.e. OUTSIDE the
device's own contract, so a device cannot state its own ingress limit
and have it enforced — it depends on whoever deploys the router. Good
mitigation today, wrong home for a contract dimension.

**2. Message priority classes (`z_priority_t`, 1 real-time .. 7
background).** zenoh carries a priority per message on the wire, the
router keeps PER-CLASS tx queues (`transport.unicast.qos.enabled` is
true by default), and zenoh-pico decodes the class — but only exposes
it to the application (`z_sample_priority`); its rx path has one FIFO
per session and never branches on it. nano-ros sets no priority at
all, so a safety-critical topic and a flood topic share the router's
`data` queue.

Tested with an env-gated knob (chain publisher at `real_time`, flood at
`background`, ~2 kHz, 3 runs each): chain delivery moved from 13-17% to
17-19% of sends, stalls and CPU were unchanged, and the harness was
noisy enough (rx rate varied 10-238 msg/s between runs) that the
delivery difference is suggestive, not evidence. The MECHANISM
conclusion is the solid part and follows from the trace, not the runs:
priority REORDERS the link, it cannot reduce the decode cost that
consumes 72-81% of the CPU, because the island still receives every
flood message. Priority is the right fix for head-of-line blocking on
the chain; it is not a fix for tier starvation.

**3. Subscriber ring depth / QoS history.** Measured above: does not
bound the stalls at all, and costs delivery. The drop happens after the
transport has already spent the CPU. Closed.

**4. Receive-window shaping (`TCP_WND`).** Measured above: bounds the
stalls perfectly (0 vs 12 per run) by starving `zpico_read` of work,
but queues the flood rather than shedding it and drives chain latency
to 446 ms. Confirms the mechanism, unusable as the fix.

**5. A budget on the drain loop itself.** `_zp_unicast_read`
(zenoh-pico, `transport/unicast/read.c`) reads a batch and then runs an
UNBOUNDED inner loop: "drain every complete frame already buffered",
added because one `recv` can pull several stream frames and the next
poll's `_z_zbuf_reset` would discard them. Under flood that loop is the
burst — it processes frames back-to-back at task priority above every
tier. A budget belongs exactly here: bound the loop by frames or by
elapsed microseconds per pass and yield, so the tiers get the CPU back
at a known granularity. Note the loop cannot simply STOP mid-buffer
(that is the bug the comment documents), so the budget has to be
"finish the frames you have, then yield before pulling more" rather
than "return with bytes unread".

### The shape this suggests

A single knob cannot do it, because two different harms are in play:

- **Tier starvation** is a CPU-volume problem. Only (1) router-side
  shedding and (5) a drain budget address it; everything else moves
  the work around.
- **Chain head-of-line blocking** is an ordering problem. (2) priority
  addresses it directly, and (5) alone would make it WORSE (deferring
  the drain leaves flood bytes queued ahead of chain bytes — the same
  effect measured with a small TCP_WND).

So the ingress dimension, if the contract grows one, wants to compile
to at least two things per subscription: a **class** (which maps to a
zenoh priority, protecting order) and a **budget** (which maps to a
drain bound, protecting cadence). That is the same split the existing
contract already makes between criticality and rate — which is a point
in its favour: the vocabulary exists, the realizer just does not emit
transport-side parameters yet.

### Open questions, in the order they block work

1. **Does the drain budget need a per-priority rx queue to be safe?**
   If the island defers draining, whatever is queued stays queued in
   arrival order. Without rx-side priority (zenoh-pico has none), a
   budget protects cadence and hurts chain latency. Either zenoh-pico
   grows per-class rx handling, or the flood must be shed before it
   arrives (1), or the two must be on separate sessions.
2. **Is per-subscription enforcement possible at all on one session?**
   The budget is naturally per-session (one read task, one socket).
   Per-subscription msg/s is what a user can state. Mapping one to the
   other needs either rx-side classification (cheap: the keyexpr is
   already decoded before dispatch) or an admission decision at
   declare time.
3. **What is the per-platform capacity input?** The ~750 msg/s drain
   envelope is specific to this lane. A budget expressed in msg/s
   cannot be validated at resolve time without a per-board capacity
   figure, which nothing currently records.

## Probe: router-side pacing (2026-08-12) — the cause is burstiness

A zenohd `downsampling` rule on the egress link to the island (raw
numbers: `results/issue506_pacing.md` in the evaluation workspace):

| cell | stalls >50 ms | worst gap | violations | rx/s | chain |
|---|---|---|---|---|---|
| uncapped (3 runs) | 9-11 | 335-511 ms | 46-62 | 157-265 | 12.8-18.5% |
| cap 50 Hz | 0 | 11 ms | 0 | 58 | 24.8-27.9% |
| cap 500 Hz | 0 | 11 ms | 0 | 442 | 29.5% |
| cap 2000 Hz (3 runs) | 0-1 | 12-93 ms | 0-4 | 693-752 | 28.8-29.9% |
| cap 5000 Hz | 7 | 395 ms | 40 | 299 | 21.1% |

Chain ceiling is ~30% by construction, so 29.8% means no loss.

Three things follow, and the second one changes the design.

1. **This is the first mechanism that fixes BOTH harms.** Cadence is
   perfect and the chain returns to its ceiling. Ring depth fixed
   neither; TCP_WND fixed cadence and destroyed chain latency; priority
   addressed ordering only.

2. **The cause is BURSTINESS, not average rate.** The controlled cell
   is the cap set AT the offered rate: 2000 Hz against ~2 kHz offered
   still removes the stalls, and the island then carries 693-752 msg/s
   versus 157-265 msg/s uncapped. It processes MORE when paced than
   when flooded — congestion collapse, not saturation. The downsampler
   enforces a minimum inter-message interval, so the island never sees
   a burst long enough to hold `zpico_read` (and therefore the tiers)
   for hundreds of milliseconds. A 5000 Hz cap (200 us interval) lets
   bursts through and the collapse returns in full.

3. **The "~750 msg/s envelope" from §8.1 is not a capacity.** Paced,
   the same island sustains 740-752 msg/s with zero stalls. That figure
   describes BURSTY traffic; the pacing interval is the variable that
   matters, which also means a msg/s budget alone cannot be validated
   against it.

### Revised design conclusion

An ingress declaration should compile to a **token bucket (rate +
burst)** rather than a rate cap, with the burst term load-bearing. The
same declaration drives two enforcement points:

- **Router-side rule** (what this probe used): sheds before the device
  spends anything, and is the only place that reduces the island's
  per-message decode cost — rx-side dropping cannot, as the ring-depth
  probe showed (dropping after decode left the stalls intact).
- **Device-side drain budget** (`_zp_unicast_read`'s unbounded inner
  loop): the same bound applied where the device can enforce it
  without trusting the deployment. Necessary because a router rule
  lives outside the device's own contract.

The two are complementary rather than alternatives: the router rule is
the only one that saves CPU, the device budget is the only one the
device controls.

### Caveat this probe makes concrete

The rule keys on a keyexpr, and in this workload the flood and the
safety chain publish to the SAME topic. The cap therefore applied to
both, and it worked only because the chain needs 10 Hz out of the
capped 50-2000. Two publishers of different criticality on one topic
cannot be separated by a router rule — that needs publisher identity in
the rule, or per-criticality topics, and it is the same limitation that
makes message priority (which IS per-publisher) complementary rather
than redundant.

## Probe: the device-side drain budget is LOSSY as posed (2026-08-14)

The design above proposed bounding `_zp_unicast_read`'s inner "drain
every buffered frame" loop. Measured with a frame cap on the 2 kHz lane
(raw numbers: `results/issue506_drain_budget.md` in the evaluation
workspace):

| cell | stalls >50 ms | miss >15 ms | viol | rx/s | chain % |
|---|---|---|---|---|---|
| unbounded (today) | 10 | 1.79% | 65 | 282 | 13.2% |
| budget = 16 frames | 4 | 0.59% | 22 | **10** | **5.7%** |
| budget = 4 frames | 5 | 0.85% | 29 | **10** | **5.4%** |
| budget = 1 frame | 12 | 1.70% | 71 | 268 | 13.2% |

Cadence does improve. It improves because messages are being **thrown
away**, not deferred:

```c
// Prepare buffer
_z_zbuf_reset(&ztu->_common._zbuf);
```

Every non-`single_read` call resets the receive buffer, so anything the
budget declines to drain is discarded on the next call — the exact
defect the unbounded loop exists to fix ("one recv can pull multiple
stream frames … a frame left here is silently lost"). The inbound drain
collapsing 282 → 10 msg/s is that loss, and chain delivery halves with
it.

A frame cap here is therefore a drop policy, not a budget.

Budget = 1 is the control: it degenerates to the pre-loop single-frame
path where the outer read task simply re-enters, so total work is
unchanged and it matches unbounded on every column.

### What this changes

- **The device-side half has a prerequisite the design did not name:**
  `_zp_unicast_read` must be able to return with bytes unread and resume
  from them. That is a change to zenoh-pico's rx state machine, not the
  one-line cap this probe used. Until then, "budget the drain" cannot be
  implemented without losing frames.
- **The head-of-line prediction is UNRESOLVED**, not confirmed. The
  probe was written to test "cadence protected, chain latency worsened";
  chain p95 barely moved (796 → 759-786 ms), but with delivery halved
  that is a different population rather than a measurement. It needs the
  non-lossy implementation.
- **Router-side pacing remains the only mechanism measured to fix both
  harms** — 0 stalls and chain at its ~30% ceiling — because it removes
  the work at the source instead of discarding it at the sink. That
  strengthens, rather than weakens, the two-enforcement-point shape: the
  router rule is not merely the one that saves CPU, it is currently the
  only one that can shed without losing what matters.


## Phase-358 W3 revisit, 2026-08-15 — the blocker cleared, and both tables went stale with it

`43ddb0ec` (zenoh-pico fork, carried by the superproject pointer) makes
`_zp_unicast_read`'s buffer reset CONDITIONAL:

```c
if (_z_zbuf_len(&ztu->_common._zbuf) == 0) {
    _z_zbuf_reset(&ztu->_common._zbuf);
} else {
    _z_zbuf_compact(&ztu->_common._zbuf);
}
```

with the commit stating the consequence directly: the unconditional reset "is
why the drain loop below has to consume every complete frame it can see, and why
a budget on that loop is lossy rather than deferring work."

**So issue 0567's conclusion — "a frame cap here is a drop policy, not a
budget" — is no longer true by construction.** An early exit now leaves the
remainder buffered for the next pass instead of discarding it. Verified by
reading the diff at the pinned commit, not inferred from the changelog.

### What this invalidates

Both measurement tables in play were taken against a receive path that no longer
exists:

* **this issue's** overload numbers (~750 msg/s sustainable drain, 100–340 ms
  transport bursts, delivery collapsing 780 → 15–260/s) — the recovery dynamics
  depend on whether a pass can leave bytes buffered;
* **issue 0567's cap table** (cap 4/16 improving cadence while collapsing
  inbound 282 → 10 msg/s) — that collapse WAS the discard. Re-running it is the
  experiment, not the baseline.

Phase 358 W3 names 0567's control as "the baseline any proposal must beat". That
needs one amendment: the control itself must be re-taken, since it too was
measured pre-`43ddb0ec`.

### The experiment, unchanged in shape

Same four columns, so the comparison stays direct:

| cell | stalls >50 ms | miss >15 ms | inbound rx/s | chain delivered |

Rows: unbounded (today), cap = 16, cap = 4, and the cap = 1 degenerate control.
The falsifiable question is narrow — **does a cap still cost delivery?** If
inbound rx/s now holds near the unbounded figure while stalls and miss% improve,
the frame cap is the budget this issue asked for and the design work reduces to
picking the cap and exposing the deferral counter. If delivery still collapses,
the loss is somewhere other than the reset and the RFC questions in this issue
stand as written.

### Not run here

The numbers come from `NEWSLabNTU/nano-ros-rt-eval` on the FreeRTOS mps2-an385
QEMU lane (icount, guest-clock timestamps); that repo is not present on this
host, and nothing in-tree measures the same columns — `nros-bench/stress-zenoh`
is a native throughput/integrity bench, not an RT stall measurement. So W3 gets
the code-level verification and the restated experiment; the table needs the
eval harness.

(Housekeeping done on the way: this checkout's zenoh-pico submodule was sitting
at `07de44fb`, one behind the recorded `43ddb0ec`, so a local build would have
tested the OLD receive path. `git submodule update` fixed it.)


## CORRECTED 2026-08-16 — the fix was on the OTHER read path

The section above says #567's conclusion is "no longer true by construction".
That was construction-only, and wrong for the lane this issue was measured on.
Recorded rather than edited away: it was written from reading a diff, and only
building the thing exposed it.

`43ddb0ec` made the reset conditional in **`_zp_unicast_read`** — the POLLED
path. `nm` on the FreeRTOS mps2-an385 image shows it exports
**`_zp_unicast_read_task`**: the `Z_FEATURE_MULTI_THREAD` path, whose
`_zp_unicast_process_peer_event` still called `_z_zbuf_reset` unconditionally at
the end of every peer's turn. On this lane a frame cap was therefore still a drop
policy, exactly as `issue506_drain_budget.md` measured (inbound 282 -> 10 msg/s),
and the prerequisite it named was NOT met.

How it surfaced: capped images came out byte-identical to the control. Each
theory was disproved in turn — the `CFLAGS_*` env var, cargo freshness, a
`cargo clean -p zpico-sys`, the issue-0475 relink trap (`libzenohpico.a` rebuilt
at 21:06 while the executable stayed at 20:59) — until an `#error` on line 1 of
`read.c` failed to fire at all, and `nm` named the function actually linked.

### Ported (zenoh-pico `f4ce3d9f`, pinned)

The task path now compacts instead of resetting **when the transport has a
single peer**. It cannot mirror the polled fix unconditionally: `_zbuf` is SHARED
across the peer list, and that reset is what stops peer A's leftover bytes being
parsed as peer B's stream. Client mode — the embedded island — is always the
single-peer case; multi-peer keeps the unconditional reset, and a budget there
would still be lossy.

Measured on the FreeRTOS lane, ~2 kHz flood, one run per cell:

| cell | stalls | worst | miss >15 ms | rx/s | chain % |
| --- | --- | --- | --- | --- | --- |
| before (unconditional) | 11 | 203 ms | 0.79% | 396 | 14.6% |
| after (conditional) | 11 | 314 ms | 1.14% | 336 | 14.3% |

Neutral, as expected — with no budget the loop drains everything, so the buffer
is empty when the reset is reached. The result that matters is that delivery did
NOT collapse: carrying a remainder across passes does not corrupt the stream.

### Still open

**Enabling work only.** There is no budget, and no four-column table. One built
on top did not differentiate cap=1/4/16 in codegen — all three produced a single
identical binary, though all three differed from unbounded — so it was dropped
rather than shipped unproven, and a cap=16-vs-4 comparison would have been
identical by construction. Next: establish why the constant does not reach
codegen, then run the cells.

Also relevant to anyone re-running this: rebuilding needs the ENTRY crate cleaned
as well as `zpico-sys`. `libzenohpico.a` rebuilds without relinking the
executable (issue 0475, here on a vendored source rather than an RMW backend),
which is why several edits silently never reached the image.


## 2026-08-16 — why the constant never reached codegen: neither cap site is in the image

Answered with `nm` on the image the cells actually run, built from current main
(`84d25f1f8`, zenoh-pico `f4ce3d9f`) on the FreeRTOS mps2-an385 lane:

| symbol | in image |
| --- | --- |
| `_zp_unicast_read_task` | **yes** (0x2ad49) |
| `_z_unicast_client_read` | yes |
| `_z_unicast_process_messages` | yes |
| `_zp_unicast_read` | **NO** |
| `_zp_unicast_process_peer_event` | **NO** |
| `_z_unicast_peer_read` | **NO** |

Statics are visible in this output (`t _zp_unicast_failed`), so the absences are
absences, not hidden local symbols.

**Both cap attempts were placed in functions this image does not contain.** That
is the whole of "the constant does not differentiate cap=1/4/16": a value in
dead code folds away, so every cap produced one binary — while all of them
differed from unbounded, because the `#if` around the *site* changed the
translation unit either way. Nothing was wrong with the plumbing.

It goes further, and this part corrects two earlier entries:

* **`43ddb0ec` (#567's conditional reset) does not apply to this lane at all.**
  It fixed `_zp_unicast_read`, the POLLED path, which is not linked here.
* **`f4ce3d9f` (the task-path port, recorded in phase-358 W3 as "done since")
  is also dead on this lane.** It lives in `_zp_unicast_process_peer_event`,
  guarded by the sole `#if Z_FEATURE_UNICAST_PEER == 1` call site, and
  `nros-zpico-build` emits `#define Z_FEATURE_UNICAST_PEER 0`. The port is
  correct and it is not reached. Its "measured neutral" result is consistent
  with that and was not evidence of anything.

### What the live path actually does

`_zp_unicast_read_task` → client branch → one `_z_unicast_client_read` +
one `_z_unicast_process_messages` per iteration of `while (_read_task_running)`.
**There is no inner drain loop on this path.** Over a TCP (stream) link
`to_read` is a single frame, so the live code already does one frame per
iteration — it is, in effect, permanently at cap = 1, losslessly, because the
buffer is never reset out from under a remainder.

So the four-column table cannot be produced as specified: unbounded, cap 16,
cap 4 and cap 1 are not four configurations of this image. There is no drain
loop here to bound.

### What this means for the issue

The premise needs restating rather than the experiment re-running. "Budget the
drain loop" was derived from `_zp_unicast_read`, and the lane the numbers came
from does not execute it. Two things follow:

1. The rx work runs on its **own FreeRTOS task**, not inline in the app. So the
   preemption measured here is a scheduling property — priority and CPU share
   between the read task and the tiers — not an unbounded loop the app calls.
   A budget is the wrong instrument for that; the right knobs are the read
   task's priority and how much it is allowed to run per period.
2. If a frame budget is still wanted for the POLLED path (multi-executor,
   `Z_FEATURE_MULTI_THREAD=0`), it is `43ddb0ec`-ready and the cells for it must
   be taken on an image that links `_zp_unicast_read` — not this one.

Not proposing either here. The item asked why the constant does not reach
codegen; it does not reach codegen because there is nothing on this lane to
apply it to, and shipping a budget against a disproved premise is what the last
two passes of this issue already did once each.

### Method note

Three passes of this issue drew a conclusion from reading a diff, and all three
were wrong: the harness was called a blocker (it clones in seconds), the
blocker was called cleared "by construction" (it was not, for this lane), and
the task-path port was recorded as the fix (it is dead code here). The thing
that settled it each time was building the image and looking at it. `nm` is
four seconds.

## Premise re-checked 2026-08-21 — the ordering holds, and the units defect behind it is FIXED

This issue opens on "the recommended FreeRTOS layout puts the transport band above every
application tier". Since the last entry that premise picked up a complication worth
resolving explicitly, because issue 0623 was about the two numbers being incomparable.

Read from the source of truth rather than a diff — `FreertosScheduling` in
`packages/boards/nros-board-common/src/freertos_config.rs`:

* the priorities are **RAW FreeRTOS**, `0..configMAX_PRIORITIES-1`, and the type says so:
  "the same units a `[tiers.<name>.freertos] priority` is written in (issue 0623)";
* the file records the fix in its own words — "They **were** on a normalized 0–31 scale,
  and **that was the defect**";
* shipped `impl Default`: `app_priority: 3`, `zenoh_read_priority: 4`,
  `zenoh_lease_priority: 4`, `poll_priority: 4`.

So the band floor is 4 and the app sits at 3: **transport above the tiers, as this issue
assumes, and now expressed in one vocabulary** so the comparison needs no arithmetic. The
0–31 conversion survives as `to_freertos_priority`, explicitly "the one place the mapping
exists", and no default path calls it.

That matters here in two ways:

1. **This issue's premise is confirmed, not weakened.** The starvation it measures is a
   consequence of a deliberate and correct ordering, not of a unit confusion — so the
   remaining design work (token bucket: class + budget) is still the right target.
2. **A reading hazard is gone.** `report_tiers_above_transport` compares two numbers in the
   same units and prints both, so anyone repeating these experiments with custom tier
   priorities gets told when they have inverted the band rather than discovering it as
   unexplained latency.

Scope note: this is a source-level verification of the defaults and the reporter's
predicate (offender iff `tier.priority >= min(read, lease, poll)`), **not** a runtime
observation — no image was booted for it. The issue's own method note is right that builds
settle things that diffs do not; nothing here contradicts a measurement, and the numbers
above are compile-time constants rather than inferred behaviour.

Corrected alongside this: CLAUDE.md's pitfall index still described the normalised scale as
current ("`zenoh_read_priority`/`poll_priority` are NORMALISED 0–31 mapped DOWN … the
defaults ship that collision"). That file is loaded every session, so it was telling every
reader a fixed collision still ships.
