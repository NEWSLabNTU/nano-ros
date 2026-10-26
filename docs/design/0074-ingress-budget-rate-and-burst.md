---
rfc: 0074
title: "Ingress budget: the contract bounds what a device is made to receive"
status: Draft
since: 2026-08
last-reviewed: 2026-10-26
implements-tracked-by: [phase-462, phase-516]
supersedes: []
superseded-by: null
---

# RFC-0074 — Ingress budget: the contract bounds what a device is made to receive

## Summary

The timing contract bounds what a device's own tasks may do, and says
nothing about what a remote publisher may send it. On a device whose
transport band sits above its application tiers — the correct default —
that omission is a hole: a peer publishing faster than the device can
drain preempts every real-time tier on it, and no local scheduling
configuration can prevent that.

This RFC proposes an **ingress declaration** on subscriptions, expressed
as a **token bucket (rate + burst)** rather than a rate cap, compiling to
two enforcement points: a **router-side pacing rule** (which is what
actually saves the device CPU) and a **device-side budget on the rx read
task** (which is what the device can enforce without trusting its
deployment). Both are measured; neither is blocked.

## Motivation

Measured on the FreeRTOS mps2-an385 QEMU lane (a 6-node safety island,
peer command chain at ~33 Hz, flood generators on the same topic), with
cadence taken from an on-target guest clock:

- Under a ~2 kHz flood the island stalls **9-11 times per 30 s run**,
  worst gap **335-511 ms**, against declared periods of 10-100 ms. Every
  application tier gaps at the same instant.
- A Tonbandgeraet trace during a burst attributes the CPU: `zpico_read`
  (the zenoh-pico session read loop) holds **72-81%**, all three
  application tiers together get **2.3-3.0%**. lwIP's `tcpip_thread`
  stays under 2% — so this is the rmw drain, not the TCP/IP stack.
- The chain the island exists to serve degrades with it: **12.8-18.5%**
  of peer commands delivered against a ~30% sampling ceiling.

The tiers are below the transport band by design, and for good reason:
the inverse ordering starves the RX drain and produces multi-second lwIP
retransmission freezes (the defect fixed in d708d8c5b). So the band
cannot be demoted — its *volume* has to be bounded instead.

### The cause is burstiness, not average rate

The controlled experiment is a router-side rate cap set **at** the
offered rate — 2000 Hz against ~2 kHz offered, which should shed almost
nothing:

| cell | stalls >50 ms | worst gap | violations | inbound rx/s | chain delivered |
|---|---|---|---|---|---|
| uncapped (3 runs) | 9-11 | 335-511 ms | 46-62 | 157-265 | 12.8-18.5% |
| cap 50 Hz | 0 | 11 ms | 0 | 58 | 24.8-27.9% |
| cap 500 Hz | 0 | 11 ms | 0 | 442 | 29.5% |
| cap 2000 Hz (3 runs) | 0-1 | 12-93 ms | 0-4 | 693-752 | 28.8-29.9% |
| cap 5000 Hz | 7 | 395 ms | 40 | 299 | 21.1% |

Capping at the offered rate still eliminates the stalls, and the island
then carries **693-752 msg/s versus 157-265 uncapped**. It processes
*more* traffic when paced than when flooded — congestion collapse, not
saturation. The pacing interval, not the mean rate, is the variable that
matters; at a 5000 Hz cap (200 µs interval) bursts pass through and the
collapse returns.

This is why the declaration must carry a **burst** term. A pure rate cap
would have been tuned to the wrong quantity.

It also demotes the "~750 msg/s drain envelope" recorded in earlier work:
paced, the same island sustains 740-752 msg/s with zero stalls. That
figure describes bursty traffic and cannot validate a msg/s budget.

## The declaration — settled in ros-launch-manifest (issue 0760)

**Status of this section: settled upstream (2026-10-17).** ros-launch-manifest
v0.1.64 added `sub.<e>.ingress: { rate_hz, burst }`, both keys required, and
play_launch carries it into the model's `SubContract.ingress`; issue 0760
(archived) records how its five points were settled. The text below is kept as
the position this RFC took into that discussion, with the declaration updated to
the spelling that shipped.

`[[subscription]]` and its field set are defined by **`ros-launch-manifest`**, a
separate repository consumed here as a TAG-pinned dependency
(`ros-launch-manifest-model` / `-sched`). nano-ros reads
`SystemModel` from it and does not own the schema. Adding `ingress` is therefore
a decision for that repo, and settling it unilaterally here would mint a field
nano-ros writes and nothing else understands — the shape RFC-0060's
two-repository amendment exists to avoid.

Issue 0760 carried the topic and the five points the discussion had to settle.

On a subscriber endpoint of the contract (`system.contract.yaml` or a node's
contract), alongside the existing rate and age fields — top level on the
endpoint, NOT under `qos`:

```yaml
nodes:
  controller:
    sub:
      control_cmd:
        ingress: { rate_hz: 200, burst: 4 }
```

- `rate_hz` — sustained inbound messages per second the device
  undertakes to absorb for this endpoint.
- `burst` — messages that may arrive back-to-back before pacing applies.
  The load-bearing half, per the measurement above. This RFC asked for an
  absent burst to default to a small constant rather than "unbounded"; the
  shipped schema goes further and makes it REQUIRED (both keys or neither),
  so an unstated burst cannot be written at all.

Both are properties of what the device can *absorb*, so they belong with
the subscription rather than in the tier table: two subscriptions on one
tier can have very different ingress costs.

**Nothing downstream waits on this.** The enforcement half takes a rate and a
burst, however they arrive: the router rule is emitted from what the resolver
already knows, and the device budget takes `(FRAMES, REST)` via the relation
below. A prototype can carry the two numbers out-of-band — an env knob or a
board fact — and lose only ergonomics. The resolve-time constraints are
arithmetic on the per-frame cost `c`, not on the schema.

## Enforcement point 1 — the router rule (saves the CPU)

The resolver emits the declaration as a zenoh `downsampling` rule
targeting the device's egress link:

```json5
downsampling: [{
  id: "<node>/<endpoint>", flows: ["egress"], messages: ["put"],
  rules: [{ key_expr: "<resolved keyexpr>/**", freq: <rate_hz> }],
}]
```

This is the only mechanism measured to fix **both** harms — cadence and
chain delivery — because it removes the work before the device spends
anything on it. Receiver-side dropping cannot: the ring-depth probe
(depth 1 vs 4) left the stalls intact at 10-12 per run while making
delivery strictly worse, because a ring drop happens *after* the
transport has decoded the message.

Two consequences worth stating plainly:

- **The rule lives outside the device.** A contract term the device
  cannot enforce alone is a new thing for this project, and the
  generated artifact has to be deployed with the router config. That is
  the price of being the only place the CPU can actually be saved.
- **A keyexpr rule cannot separate two publishers on one topic.** In the
  measured workload the flood and the safety chain share a topic, so the
  cap applied to both; it worked only because the chain needs 10 Hz out
  of the capped 50-2000. Per-publisher separation needs either publisher
  identity in the rule or per-criticality topics.

### As lowered (amendment 2026-10-26, phase-516 W1)

Read against the router we actually run — ROS's `rmw_zenohd`, linking zenoh
1.10.1 — the sketch above needs four corrections. Each is a property of
zenoh's `downsampling` interceptor (`zenoh/src/net/routing/interceptor/
downsampling.rs`), not a choice.

1. **The router lowers the declaration at burst 1.** The interceptor keeps one
   timestamp per rule and per transport and drops a message that arrives less
   than `1/freq` after the last one it passed: a minimum interval, which is a
   token bucket whose depth is fixed at one. There is no burst field. So
   `ingress: { rate_hz, burst }` lowers to `freq = rate_hz` and the burst is
   NOT carried. That sheds messages inside a burst the device declared it
   could absorb — safe for the device, lossy for the sender. This does not
   contradict "a pure rate cap, no burst term" under Rejected alternatives:
   that rejects the cap as the DECLARATION. The declaration keeps its burst,
   and enforcement point 2 (the device budget) still compiles it in full; only
   the router's half is narrower, and the probe that motivates it measured
   exactly this mode. A token-bucket mode is an upstream zenoh request; until
   it lands, burst 1 is the router's whole vocabulary.
2. **The keyexpr names the type segment and wildcards only the hash.** The
   rule is `<domain>/<topic>/<pkg::msg::dds_::Name_>/*` — the subscriber
   wildcard `TopicKeyExpr::to_key_wildcard` already builds. `<keyexpr>/**`
   would also match a deeper topic (`/a/b` under `/a`), and the hash segment
   differs by edition (`TypeHashNotSupported` on Humble, `RIHS01_…` on Jazzy),
   so `*` there makes one rule serve both. A bringup whose images resolve to
   several domains gets one rule per domain.
3. **A rule cannot name one device's link.** Its only selectors are
   `interfaces` (host NIC names) and `link_protocols` (`tcp`, `serial`, …);
   there is no peer (zid) selector. A rule with neither applies to the
   router's egress toward EVERY peer — every peer separately, since state is
   per transport, but a ROS node subscribed on the same router is capped too.
   So the producer emits an untargeted rule by default and says so, and the
   deployment answer is an EDGE router per device (or per device class).
   Measured (phase-516 W1b): the rule only sees traffic the router FORWARDS.
   Two `rmw_zenoh_cpp` nodes in their default peer mode linked directly and
   a 50 Hz rule left a 500 Hz stream at 500 Hz; in client mode the same pair
   read 47 Hz. A zenoh-pico device is a client, so its traffic is always
   routed — the reach that matters — while a host peer-to-peer pair is not
   capped by a rule it never crosses. Two
   selectors are emitted when a fact supports them: `link_protocols` when the
   image's own locator scheme is distinctive (a serial link is), and an
   `interfaces` list an image states as a deployment fact. Nothing infers a
   host NIC name.
4. **Several subscribers on one topic get the LOWEST rate.** The router sends
   one stream per topic per transport, so the rule must be the tightest
   declaration among them; the producer keeps the per-endpoint provenance in
   a comment so the winner is visible.

The artifact is per resolved model, beside it:
`build/nros/models/<bringup>/<model-stem>.router.json5`, written by `nros sync`
only when some subscriber declares an ingress (no fact, no file; a stale one is
deleted). `rmw_zenohd` takes no config path on its command line, so the
deployable form is also printed as a `ZENOH_CONFIG_OVERRIDE` string by
`nros ws router-config`, which `just <platform>::zenohd <model>` runs.
A dropped message is visible at the router as its own info line, `Some
message(s) have been dropped by the downsampling interceptor` — that is the
"drop counter" open question 4 asks for, at the granularity zenoh offers: it
says THAT a rule shed, not how much.

## Enforcement point 2 — a budget on the read TASK (measured, unblocked)

The device half bounds the **read task's loop** — `_zp_unicast_read_task`,
which is the loop the image actually executes — pausing after N frames so
the tiers regain the CPU at a known granularity.

```c
if (++frames >= NROS_RX_TASK_BUDGET_FRAMES) { frames = 0; z_sleep_us(rest); }
```

**Shipped (phase-462 W6a) as a stated knob:** `NROS_ZENOH_RX_TASK_BUDGET_FRAMES`
and `NROS_ZENOH_RX_TASK_BUDGET_REST_US` on the plain ladder (env, Kconfig
`CONFIG_NROS_ZENOH_RX_TASK_BUDGET_*`, `[knobs.zenoh.wire]` in a board or platform
TOML), lowered to zenoh-pico's `Z_RX_TASK_BUDGET_FRAMES` / `_REST_US` in
`_zp_unicast_read_task`. Unset or -1 bakes no define, so an image that states
nothing is byte-identical to before; a stated 0 is refused on every road. The
derivation from a declared `(rate_hz, burst)` is phase-462 W6b.

**Derived (phase-462 W6b):** the entity inventory takes the subscriber with the
largest declared `rate_hz x burst` and states `FRAMES = burst`,
`REST = round(burst x 10^6 / rate_hz)` — this section's relation with `c = 0`,
because no per-board frame cost exists in the tree yet. The admitted rate is
`FRAMES x 10^6 / (REST + FRAMES x c)`, so with `c = 0` the budget's nominal rate
(the declared one) is an UPPER bound of what the board actually admits: the task
UNDER-admits by the per-frame cost, and the excess waits in the socket. REST is clamped into the knob's range
(`1..=1000000` us). The pair reaches the Zephyr resolver as
`NROS_DERIVED_ZENOH_RX_TASK_BUDGET_*` (a rung below Kconfig and the environment)
and every cargo road as the sizing descriptor's `[image] rx_task_budget_*`
(a rung below every stated one in `nros-zpico-build`). No ingress declared: no
derivation, an unbounded task, a byte-identical image.

**Per-board `c` and the checks (phase-462 W6c):** a board states its measured
cost as `[board.rx] frame_cost_us` (and optionally `floor_us`) in
`nros-board.toml` — a fact, not a knob. Every road that derives the pair hands
the inventory its board's `c`, and the derivation becomes this section's
relation in full, `REST = burst x (10^6/rate_hz - c_us)`; the two constraints
of "What validates a declaration" are checked at resolve time and a failing
declaration REFUSES, naming the subscriber, the board fact and the remedy. A
board that states no `[board.rx]` keeps `c = 0`, checks nothing, and its image
is byte-identical to W6b. `nros-board-mps2-an385-freertos` states this RFC's
measurement (940 us, 11 ms floor).

Two structural properties make this a budget rather than a drop policy,
and both were checked against the source before it was built:

- **`_z_zbuf_reset` is called once, ABOVE the loop**, not per iteration.
  Pausing between frames therefore *defers*: the bytes stay in the socket
  and TCP backpressures the sender.
- **`_mutex_rx` is held for the task's whole life with no other locker**,
  so sleeping while holding it blocks nothing in steady state.

Measured on the FreeRTOS mps2-an385 lane, ~2 kHz flood, **six runs per
cell, interleaved** (`results/issue506_task_budget.md`, eval workspace):

| cell | n | stalls/run | worst | chain % | chain p50 | chain p95 |
|---|---|---|---|---|---|---|
| unbounded | 6 | 9.0 | 610 ms | 10.2 | 38 ms | 842 ms |
| budget 8 | 6 | **0.0** | **21 ms** | 11.6 | 95 ms | 911 ms |
| budget 32 | 6 | **0.0** | **38 ms** | 12.4 | 38 ms | 892 ms |

12/12 budgeted runs have zero stalls against 6/6 unbounded runs with
2–13; worst gap separates by an order of magnitude with no overlap and is
dose-responsive (8 < 32). Chain delivery is flat-to-better, so this is
not the drop policy; chain p95 is flat across cells, so it is not
`TCP_WND` either.

**Not claimed:** throughput (per-run rx spans 10–1047 msg/s inside one
cell) and chain p50 (38/95/38, non-monotonic). Tuning — frames and rest
interval — is unexplored; the cells above use 1000 µs.

### Rerun on the shipped knob (2026-10-18, phase-462 W6a)

The same lane on the knob as it shipped (`NROS_ZENOH_RX_TASK_BUDGET_FRAMES`,
rest 1000 µs), the island ported to the current tree, ~2 kHz from four
generators, six interleaved runs per cell, `analyze_506.py` `cell_stats`:

| cell | n | stalls/run | worst | rx/s median | chain % | chain p50 |
|---|---|---|---|---|---|---|
| unbounded | 6 | 0.33 | 206 ms | 649 | 24.8 | 20 ms |
| budget 8 | 6 | **0.0** | **11.5 ms** | **1,250** | 25.4 | 22 ms |

6/6 budgeted runs are stall-free with a worst gap of 10.8-11.5 ms in every
run. Two things differ from the 2026-08 table, and both are stated rather
than smoothed:

- **Unbounded stalls less often** — 1 run of 6 (2 stalls, 206 ms), not 6 of 6.
  The stall count now separates the cells weakly; the worst gap and the
  throughput separate them with no overlap.
- **Throughput is now claimed**: the budget nearly doubles what the island
  drains (649 -> 1,250 msg/s median), the "processes more when paced" result
  of the Motivation reproduced by the task budget alone.

The rerun is valid only after two defects outside the budget were fixed, and
both had invalidated earlier attempts: the eval driver blocked its own load
generators on an undrained stdout pipe (a hosted image logged a line per
publish — issue 1949), and QEMU's LAN9118 fast-forward leaked RX FIFO space
on every pbuf-pool drop, so above ~1 kHz BOTH cells stopped receiving for good
(issue 1948, fixed in the drivers). Whether the 2026-08 stalls were partly the
second defect is not known: those runs recovered, which a full leak does not.

### Correction: the previous blocker was aimed at dead code

Earlier drafts said this half was blocked on issue
[#0567](../issues/archived/0567-zpico-rx-cannot-resume-partial-buffer.md),
a resumable rx path, on the strength of a probe that capped
`_zp_unicast_read`'s inner loop and measured delivery collapsing
282 → 10 msg/s.

`nm` on an image from that same lane shows `_zp_unicast_read` is **not
linked** (`_zp_unicast_read_task` is), so the probe capped a function the
image does not contain, and #0567's fix (`43ddb0ec`) lands in the same
absent function. The old cells are consistent with that rather than with
their causal story: caps 16 and 4 were identical on the discriminating
column, and cap 1 was indistinguishable from unbounded — one binary
measured three times. That probe's conclusion is therefore unsupported
rather than wrong, and the blocker it produced never applied to this lane.

Chain/flood **separation** was likewise proposed as a precondition, on
the expectation that deferring the drain would relocate the harm into
head-of-line delay. At this load it did not (chain p95 flat). Separation
is not refuted — the p50 figure that would settle it is unresolved — but
it is not blocking.

## Interaction with message priority

zenoh carries a priority class per message and the router keeps per-class
tx queues; nano-ros sets none, so a safety-critical topic and a flood
topic share the router's `data` queue. Priority is the right tool for the
*ordering* harm (head-of-line blocking of the chain behind flood bytes)
and is complementary to this RFC rather than an alternative: it reorders
the link, it cannot reduce a decode cost the device pays per message.

Mapping the contract's criticality tiers onto zenoh priority classes is
left to a follow-up; an env-gated A/B moved chain delivery 13-17% →
17-19% but was too noisy on an ad-hoc harness to claim.

## Rejected alternatives

- **Subscriber ring depth / QoS history.** Measured: does not bound the
  stalls, and costs delivery. The drop happens after the CPU is spent.
- **Receive-window shaping (`TCP_WND`).** Bounds the stalls perfectly
  (0 vs 12 per run) by starving the drain, but queues rather than sheds
  and drives chain latency to 446 ms p50. Confirms the mechanism,
  unusable as the fix. (Caveat added 2026-08-21: that p50 is two runs of
  a statistic later shown to vary ~8x run-to-run on an unchanged
  configuration — see open question 6. 446 ms is outside the range
  observed since, so the result is probably real, but it is weaker
  evidence than it reads.)
- **Demoting the transport band below the tiers.** This is the
  configuration d708d8c5b fixed; it starves the RX drain into
  multi-second RTO freezes.
- **A pure rate cap, no burst term.** Refuted by the cap-at-offered-rate
  cell above.

## What the budget actually bounds — occupancy, not rate

Established 2026-08-21 by a frames sweep, and it settles several questions
below at once. A budget of N frames then a 1 ms rest permits ~N x 10^3 msg/s,
an order of magnitude above the ~750 msg/s this island can drain — so the
budget is almost never the binding RATE constraint. What it bounds is the
CONTIGUOUS run of the read task, which is exactly the harm the trace
attributed (bursts holding `zpico_read` for 100-340 ms).

The sweep confirms it. Worst observed gap against the frame budget:

| frames | 8 | 32 | 128 | unbounded |
|---|---|---|---|---|
| worst gap | 21 ms | 38 ms | 132 ms | 610 ms |
| stalls/run | 0.0 | 0.0 | **12.5** | 9.1 |
| n | 12 | 6 | 6 | 12 |

Worst gap is linear in frames at roughly **0.8-1.0 ms per frame** plus a ~14 ms
floor. That is a per-frame processing cost, and it is the per-platform number
this design needs.

**A budget can be set too large.** At 128 frames the stall COUNT is worse than
unbounded (12.5 vs 9.1) while the worst gap is far better (132 vs 610 ms): it
bounds occupancy, but not below the 50 ms stall threshold, so rare huge bursts
become regular moderate ones. The knob is only useful when
`frames x per_frame_cost` fits inside the tightest tier's slack.

## Compiling the declaration — `(rate_hz, burst)` to `(FRAMES, REST)`

Measured 2026-08-21 by a frames sweep; this closes questions 2, 4 and 5 with one
relation.

The mechanism takes FRAMES messages back-to-back, then idles REST. So over one
cycle:

    max contiguous occupancy = FRAMES x c
    sustained rate           = FRAMES / (FRAMES x c + REST)

where `c` is the **per-frame processing cost**, the one number a platform has to
record. On the FreeRTOS mps2-an385 lane, worst observed tier gap against the
frame budget fits

    worst_gap(ms) = 0.940 x FRAMES + 11.0        (measured 21 / 38 / 132 at
                                                  FRAMES = 8 / 32 / 128;
                                                  fitted 19 / 41 / 131)

so `c` = 0.94 ms there. Two independent checks support it: the fit predicts the
observed pass/fail boundary (32 frames -> 41 ms, no stalls; 128 -> 131 ms,
stalls, worse than unbounded on stall COUNT), and `1/c` = ~1060 msg/s brackets
the 740-752 msg/s this same island sustained under router pacing — a different
experiment.

### The mapping

- **`burst` -> FRAMES**, directly. `burst` is defined as the messages that may
  arrive back-to-back before pacing applies, and FRAMES is exactly how many are
  taken before the task yields.
- **`rate_hz` -> REST**, by solving the rate equation:

      REST = burst x (10^6 / rate_hz - c_us)

### What validates a declaration (question 2)

Both constraints fall out of the same two lines, and both are per-platform via
`c`:

1. **Feasible at all.** `REST >= 0` requires `rate_hz <= 1/c` — about 1060 msg/s
   on this lane. A larger `rate_hz` is not a policy the device declines, it is
   arithmetic the device cannot satisfy.
2. **Does not itself miss a deadline.** `c x burst + floor <= slack` of the
   tightest tier sharing the core. This is the constraint FRAMES = 128 violates:
   it bounds occupancy (131 ms vs 610 ms unbounded) but not below the 50 ms
   threshold, so rare huge bursts become regular moderate ones.

Both are resolve-time checks against numbers the contract already carries —
tier periods and deadlines — plus `c`.

**As implemented (phase-462 W6c).** Constraint 1 is checked for EVERY declared
subscriber, not only the heaviest. Constraint 2 uses the derived FRAMES (the
heaviest subscriber's burst — the run the read task actually takes) and the
tightest AUTHORED tier, with `slack = D - B`: `D` the tier's `deadline`, else
its `period`; `B` its `budget`. A tier that states no `budget` contributes its
whole deadline — an upper bound of its slack, so a refusal is sound and a pass
is reported as "within the deadline". Every tier is read as sharing the read
task's core (the model places none), the stricter reading; a platform
sub-table's tighter deadline is not read. An image whose tiers state no
`deadline`/`period` has no slack to compute: constraint 2 is reported as NOT
CHECKED, with the reason, rather than checked against a guessed slack. An
absent `floor_us` is taken as 0, which can only under-state the occupancy, and
the refusal says so.

### Caveats

`c` and the 11 ms floor are from ONE lane, ONE payload size, and three budget
points. The floor is unexplained (plausibly the tier's own work plus scheduler
latency) and is carried as an empirical term rather than modelled. Message size
almost certainly enters `c` — a larger payload costs more to decode — so `c` may
need to be per (platform, type) rather than per platform; nothing here measures
that, and a design that assumes one `c` per board should say so.

## Open questions

1. ~~**Per-subscription declaration, per-session enforcement.**~~ **Largely
   dissolved.** CPU occupancy is inherently a per-SESSION property — one read
   task, one socket — so per-session enforcement is the correct scope rather
   than an implementation compromise. The declaration stays per-subscription
   because that is what a user can state, and the resolver aggregates. What
   remains open is only whether SHEDDING wants per-endpoint attribution, and
   shedding lives at the router (enforcement point 1).
2. ~~**What validates `rate_hz` at resolve time?**~~ **Answered — two
   constraints, both from one per-platform number.** See "Compiling the
   declaration" below. The number to record is the **per-frame processing cost
   `c`** (0.94 ms on this lane), and a platform records exactly one of it.
   Remaining work is mechanical: measure `c` per platform and store it beside
   the other board facts. Since phase-462 W6c the home exists (`[board.rx]`)
   and the mps2-an385 FreeRTOS board states this lane's measurement; other
   boards are unmeasured.
3. ~~**Does BEST_EFFORT already express part of this?**~~ **Answered: no.**
   Checked in zenoh-pico: `reliability` is a PUBLISHER-side field
   (`publisher->reliability`, stamped per message on the tx path), and
   `_z_declare_subscriber` takes keyexpr, callback, dropper, arg and
   `allowed_origin` — no QoS or reliability parameter. `_z_subscriber_t` holds
   only an entity id and a session handle. A subscriber therefore cannot signal
   "shed for me"; there is no field on the declare to carry it. BEST_EFFORT is
   not a partial answer to this RFC.
4. ~~**Failure mode.**~~ **Answered, and the two points differ.** Reject at
   resolve time is now possible rather than aspirational, because question 2
   supplies a bound: a declaration failing either constraint below is
   arithmetically unsatisfiable, not merely ambitious, so it is a hard error.
   Admitted declarations then differ by enforcement point — the router rule
   SHEDS and needs a drop counter; the device budget sheds NOTHING (it defers,
   and TCP backpressures the sender, because `_z_zbuf_reset` is above the loop),
   so its observable is occupancy or deferral time. The RFC should not ask one
   question of both.
5. ~~**How the declaration compiles to the task budget.**~~ **Answered.** See
   "Compiling the declaration" below — it is arithmetic, once `c` is known.
6. ~~**Does the budget cost chain latency?**~~ **Answered: no detectable
   cost**, and the p50 figures that suggested one were an artifact of the
   statistic.

   The chain-latency distribution is **bimodal** — p25 is ~24 ms and p75 ~400 ms
   in every cell, with little between — so p50 lands in the valley and reports
   the MIX RATIO rather than a latency. p10 and p25 are identical across cells,
   meaning the fast path is untouched, which a queueing delay could not leave
   alone.

   Measured by mode proportion instead (fraction under 100 ms, per run):
   unbounded 57.5 %, budget 8 50.6 % (Welch t +1.36), budget 32 61.3 % (-0.76),
   budget 128 53.2 % (+0.60). All |t| < 2, per-run ranges overlap almost
   entirely, and the signs disagree — budget 32 is nominally better than
   unbounded. At 12 runs per cell with a per-run SD of 8-16 points, there is no
   effect to see.

   **Consequence: chain/flood separation is not required alongside the budget**
   on this evidence. It stays a legitimate design for the ORDERING harm
   (priority reorders the link), but it is not a precondition for the device
   half.

   Two caveats belong with any future chain-latency claim here, and neither is
   about the budget: `chain_lat` is **censored at 1 s** by the analyzer's
   matching window (which is why p99 is ~974 ms in every cell), and p50 is
   unstable by construction on this distribution — cells reported with 1-2 runs,
   including `TCP_WND 1xMSS`'s 446 ms, carry less weight than their tables
   imply.

## Changelog

- 2026-10-26 — phase-516 W1: "As lowered" added under enforcement point 1 —
  the router lowers the declaration at burst 1 (zenoh's interceptor has no
  burst term), the rule's keyexpr wildcards only the hash segment, a rule
  cannot select one peer so the default is untargeted plus an edge-router
  deployment, and co-subscribers take the lowest rate.

- 2026-10-18 — phase-462 W6a: the flood cell rerun on the shipped knob,
  6/6 budgeted runs stall-free (worst 11.5 ms) and 1.9x the unbudgeted
  throughput; recorded under "Rerun on the shipped knob" with the two
  defects (issues 1948, 1949) that had invalidated the earlier attempts.
- 2026-10-18 — phase-462 W6c: the per-board `c` (`[board.rx]`) and both
  resolve-time constraints landed; "As implemented" under "What validates a
  declaration" records the readings the RFC left open (every subscriber for
  constraint 1, the tightest authored tier and `D - B` for constraint 2, NOT
  CHECKED when no tier states timing).
- 2026-08 — initial draft, from the #0506 investigation (trace
  attribution, router pacing probe, drain budget probe).
- 2026-08-23 — the DECLARATION is marked a proposal pending a
  `ros-launch-manifest` discussion (issue 0760): that repo owns
  `[[subscription]]`, so the field set is not nano-ros's to decide. The
  enforcement half, the occupancy model and the compile relation are unaffected
  and remain measured.
- 2026-08-21 — enforcement point 2 rewritten. Its mechanism changes from
  `_zp_unicast_read`'s inner drain loop to a budget on the read TASK, and
  it is no longer blocked: `nm` shows the old target is not linked on the
  lane every measurement came from, so both stated prerequisites (#0567's
  resumable rx, and chain/flood separation) were derived from mechanisms
  that were not the one running. The replacement is measured over six
  interleaved runs per cell — zero stalls in 12/12 budgeted runs,
  dose-responsive, with chain delivery and p95 intact.
