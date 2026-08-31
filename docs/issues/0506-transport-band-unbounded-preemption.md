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

Tooling: `just freertos trace <load>` and `tools/analyze_trace_cpu.py`
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

## Independent confirmation 2026-08-21 — `nm` on a shipped image of this lane

The evaluation workspace (`NEWSLabNTU/nano-ros-rt-eval`) is now cloned locally, so
the 2026-08-16 dead-code finding above could be checked against an artefact
rather than re-derived. It holds, by a different method than the one that
produced it.

`results/msweep-20260808-043004Z/img-baseline-7449.elf` — a FreeRTOS
mps2-an385 image from the lane every number in this issue comes from:

```
_zp_unicast_read               0     <- the function the drain budget capped
_zp_unicast_read_task          1     <- the live path
_z_unicast_client_read         0     (inlined into the task)
_zp_unicast_process_peer_event 0
```

Two consequences, both of which move things that are currently treated as
settled:

### 1. The drain-budget probe measured a constant it could not reach

`results/issue506_drain_budget.md` in the evaluation workspace reports a
four-cell table (unbounded / 16 / 4 / 1 frames) and concludes that "a frame cap
in this rx structure is not a budget at all. It is a drop policy wearing a
budget's clothes." The cap was `NROS_DRAIN_BUDGET_FRAMES` on
`_zp_unicast_read`'s inner loop — and that function is not in the image.

The table's own shape is consistent with this rather than with the causal story:
cells 16 and 4 are IDENTICAL on the discriminating column (rx 10 and 10), which
is what one binary measured twice looks like, and cell 1 is indistinguishable
from unbounded (rx 268 vs 282, chain 13.2% vs 13.2%). This issue elsewhere
records the harness varying rx between 10 and 238 msg/s across runs of an
unchanged build, and each cell here is a single 30 s run.

So the conclusion is unsupported — not wrong, unsupported. A frame cap may well
be lossy in that structure; this experiment cannot say so, because it never
compiled a difference.

### 2. Issue 0567's fix does not reach this lane either

0567 is closed as "zenoh-pico fork `43ddb0ec` — reset only when the buffer is
empty", and its title is exactly the precondition the drain-budget result asks
for: a receive buffer that survives across passes. But `43ddb0ec` fixed
`_zp_unicast_read`, which the `nm` above shows is absent. The resumable rx is
real and it is not linked here.

That matters for RFC-0074: its device-side half rests on bounding a drain loop
made non-lossy by 0567, and on the platform that produced all of this issue's
evidence there is no such loop and no such fix in the image. The live path takes
one frame per iteration of its own task already.

### What this leaves standing

* **Router-side pacing is still the only mechanism measured to fix both harms**
  (`issue506_pacing.md`), and nothing here touches it. It also does not depend
  on the device's rx structure, which is now the more robust half of the design.
* **The band that preempts the tiers is still `zpico_read` at 72-81 % CPU** —
  that came from a Tonbandgeraet trace, not from the cap experiment.
* **The device-side half needs re-aiming**, at the task the trace names rather
  than the loop the source reading suggested: the read task's priority and CPU
  share per period. That is a scheduling instrument, and this issue already has
  one — `report_tiers_above_transport` compares the two in one vocabulary.

### Method

This is the fourth pass of this issue where reading source produced a
conclusion that an artefact then contradicted, and the issue's own note predicted
it: "`nm` is four seconds." It was. The cost of the previous three was two
recorded results and one closed issue aimed at dead code.

## Design options for the device-side half (2026-08-21)

With `_zp_unicast_read` shown absent from the image, RFC-0074's enforcement
point 2 needs re-aiming rather than unblocking. What follows is the option set,
with the two structural facts that decide most of it read off the LIVE path
(`_zp_unicast_read_task`, `src/transport/unicast/read.c:386`):

```c
_z_mutex_lock(&ztu->_common._mutex_rx);   // acquired and KEPT for the task's life
_z_zbuf_reset(&ztu->_common._zbuf);       // ONCE, before the loop
while (running) { _z_unicast_client_read(...); _z_unicast_process_messages(...); }
```

* **The reset is outside the loop.** This is the whole difference from the failed
  probe: a cap on `_zp_unicast_read`'s INNER loop was lossy because that
  function resets per call, so declined frames were discarded. Pausing the TASK
  between iterations discards nothing — bytes stay in the socket and TCP
  backpressures the sender.
* **`_mutex_rx` is held for the task's lifetime, and nothing else locks it** —
  elsewhere in the tree it is only `_z_mutex_init` / `_z_mutex_drop`. So
  sleeping inside the loop blocks nothing in steady state. (Teardown already
  clears `_read_task_running` and joins.)

### A. Lower the read task below the tiers — REJECTED, with evidence

The obvious move, and measured bad: this is the configuration that starved the
RX drain and froze `rt-eval`'s island for 1-3 s on lwIP retransmission. Kept in
the option set only so it stops being re-proposed; `report_tiers_above_transport`
exists to catch someone arriving at it by accident.

### B. Budget the read TASK (the re-aimed enforcement point 2)

Bound the task by frames or microseconds per replenishment period and block
until the next one — a sporadic server in application clothing, applied to the
task loop rather than to an inner loop that does not exist here.

Viable on the two facts above: non-lossy, and mutex-safe in steady state. Its
real cost is the one the TCP_WND experiment already measured: deferring the
drain leaves flood bytes queued ahead of chain bytes on one reliable ordered
stream, which converts CPU preemption into head-of-line delay (1xMSS: cadence
perfect, chain p50 45 ms -> 446 ms). **So B alone relocates the harm.**

One difference from TCP_WND worth testing rather than assuming: a budget engages
only ABOVE the declared rate, where the 1xMSS window was permanently small. Under
nominal traffic B is inert. Whether that makes the head-of-line cost acceptable
in practice is unmeasured, and is the experiment B needs before it ships.

### C. Separate the chain from the flood — the actual precondition

Priority classes, per-criticality topics, or separate sessions. Priority
REORDERS the link and cannot reduce a per-message decode cost, so it does not
substitute for a budget; but it is what makes B help instead of relocate.

**This replaces #0567 as enforcement point 2's blocker.** RFC-0074 currently
states the precondition as "a resumable rx path (#0567)". On this lane #0567's
fix (`43ddb0ec`) is in `_zp_unicast_read` and is not linked, and there is no
inner loop to resume. The precondition that actually binds is separation.

That is a better place to be blocked: #0567 is a change to zenoh-pico's rx state
machine, whereas separation is a topology and contract question this project
already has vocabulary for.

### D. Ship enforcement point 1 alone

Router-side pacing is the only mechanism measured to fix BOTH harms, and it does
not depend on the device's rx structure — which today's finding makes the more
robust half. The RFC could make EP1 normative and EP2 an explicit non-goal until
B has evidence.

Cost, and it is real: a device cannot then enforce its own ingress contract, and
the rule ships outside the device with whoever deploys the router. The RFC
already names this as the price of being the only place the CPU is saved.

### E. Admission control at resolve time — complementary, currently unfounded

Reject a declared ingress that exceeds the platform's capacity. Cheap and it
catches misconfiguration rather than overload. Blocked on a definition: this
issue established that "~750 msg/s" is NOT a capacity — paced, the same island
sustains 740-752 msg/s with zero stalls, and unpaced it collapses to 157-265.
Capacity is a (rate, burst) pair, and nothing measures one today.

### Recommendation

D now, B+C as the follow-up, in that order — because D is the only thing with
evidence behind it, and B without C is measured to move the harm rather than
remove it. The RFC edit this implies is small: EP2's blocker changes from #0567
to separation, and its mechanism changes from the inner drain loop to the read
task's budget.

The experiment that would settle B is also now well-posed, which it was not
before: run the task-level budget with the chain and the flood on separate
sessions, and compare chain p50/p95 against the 1xMSS cell. That needs an image
plus the eval workspace's 3-phase harness, not a new instrument.

## Option B MEASURED 2026-08-21 — it works, and my own recommendation above was wrong

The design options section recommended "D now, B+C as follow-up", on the
reasoning that B without C would relocate the harm into head-of-line delay.
**Measured, that does not happen**, and B alone removes the harm this issue was
filed about.

Full write-up and raw numbers: `results/issue506_task_budget.md` in the
evaluation workspace (`15e7f2a`). Method: FreeRTOS mps2-an385 QEMU icount,
3-phase demo, ~2 kHz flood, **six runs per cell, interleaved A/B/C** so host
drift cannot masquerade as a cell effect; metrics computed with
`analyze_506.py`'s own `cell_stats`, so they are comparable to every other cell
recorded here.

| cell | n | stalls/run | worst | rx/s med | chain % | chain p50 | chain p95 |
|---|---|---|---|---|---|---|---|
| unbounded | 6 | 9.0 | 610 ms | 249 | 10.2 | 38 ms | 842 ms |
| budget 8 | 6 | **0.0** | **21 ms** | 899 | 11.6 | 95 ms | 911 ms |
| budget 32 | 6 | **0.0** | **38 ms** | 386 | 12.4 | 38 ms | 892 ms |

```
unbounded  stalls [10, 8, 11, 13, 2, 10]  worst [411, 346, 330, 610, 308, 511]
budget 8   stalls [ 0, 0,  0,  0, 0,  0]  worst [ 16,  19,  20,  16,  16,  21]
budget 32  stalls [ 0, 0,  0,  0, 0,  0]  worst [ 34,  26,  26,  38,  29,  38]
```

### What the numbers support

* **The starvation is gone.** 12/12 budgeted runs have zero stalls against 6/6
  unbounded runs with 2-13. No overlap, and worst-gap is tight WITHIN each cell
  (16-21, 26-38) unlike every throughput figure this harness produces.
* **Dose-response.** budget 8 < budget 32 on worst gap. The failed inner-loop
  probe could not produce this, because its caps all folded to one binary.
* **Not the drop policy.** Chain delivery flat-to-better (10.2 -> 11.6/12.4 %),
  against the inner-loop cap's halving (13.2 -> 5.7 %). That follows from the
  structural fact checked before building: `_z_zbuf_reset` runs ONCE above the
  loop, so a pause defers rather than discards, and TCP backpressures the sender.
* **Not TCP_WND either.** Chain p95 is flat across cells (842/911/892 ms);
  1xMSS bought its cadence at chain p50 446 ms.

### What is NOT claimed

Throughput — per-run rx has spanned 10-1047 msg/s inside a single cell, so the
favourable medians mean nothing at this sample size. And chain p50 (38 / 95 /
38) is non-monotonic: budget 8's 95 ms is either noise or an effect this design
cannot separate from it. **That is precisely the number that would justify
session separation**, so C is not refuted — it is unmeasured, and no longer
blocking.

### Consequence for RFC-0074

Enforcement point 2 has now had two prerequisites removed by measurement rather
than by argument:

1. **Not #0567.** Its fix (`43ddb0ec`) is in `_zp_unicast_read`, absent from the
   image; the RFC's stated blocker never applied to this lane.
2. **Not separation (C), on this evidence.** The head-of-line cost that made C a
   precondition did not appear at ~2 kHz on a shared session.

So the device-side half is implementable as a bound on the READ TASK, and the
RFC's mechanism should change from the inner drain loop to that. What remains
open is tuning (frames and rest are fixed at 8/32 and 1000 us, untuned) and the
chain p50 question above.

### Scope

One lane, one load, one run shape, fixed rest interval. A stricter chain
deadline, a higher load or another platform could change it. Also: this is a
probe patch in the vendored zenoh-pico, not a shipped change — the patch is in
the results doc so it is reproducible, and nothing was pushed to the fork.

## Scope split 2026-08-23 — the declaration is parked, the enforcement is not

The remaining work on this issue divides along a repository boundary, and only
one half is nano-ros's to finish.

* **The `ingress` declaration** (`[[subscription]] ingress = { rate_hz, burst }`)
  is a `ros-launch-manifest` schema change — that repo defines `[[subscription]]`
  and nano-ros consumes it TAG-pinned. Parked as **issue 0760** pending that
  discussion, at the maintainer's direction. RFC-0074's declaration section now
  says so at the point of use rather than reading as settled.
* **Everything measured stands.** The router rule, the read-task budget, the
  occupancy model (`worst_gap = 0.94 x FRAMES + 11 ms`) and the two resolve-time
  constraints are independent of how the term is spelled. The budget takes
  `(FRAMES, REST)`; any source of a rate and a burst compiles to them.

So this issue is no longer blocked on a design question. It is blocked on
implementation, and the implementable part is the enforcement half — which could
proceed against an out-of-band source of the two numbers if that is ever wanted
before the schema lands.
