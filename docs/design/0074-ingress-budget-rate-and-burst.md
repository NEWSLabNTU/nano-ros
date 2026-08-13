---
rfc: 0074
title: "Ingress budget: the contract bounds what a device is made to receive"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: []
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
actually saves the device CPU) and a **device-side drain budget** (which
is what the device can enforce without trusting its deployment).

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

## The declaration

On a subscription, alongside the existing QoS and contract fields:

```toml
[[subscription]]
topic = "/control/command/control_cmd"
ingress = { rate_hz = 200, burst = 4 }
```

- `rate_hz` — sustained inbound messages per second the device
  undertakes to absorb for this endpoint.
- `burst` — messages that may arrive back-to-back before pacing applies.
  The load-bearing half, per the measurement above. Absent, it defaults
  to a small constant (1-4), NOT to "unbounded" — an unstated burst is
  the failure mode this RFC exists to close.

Both are properties of what the device can *absorb*, so they belong with
the subscription rather than in the tier table: two subscriptions on one
tier can have very different ingress costs.

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

## Enforcement point 2 — the device-side drain budget (blocked)

The device half bounds `_zp_unicast_read`'s inner drain loop, so the read
task returns and the tiers regain the CPU at a known granularity.

**This cannot be implemented today**, and the reason is recorded as
issue [#0567](../issues/0567-zpico-rx-cannot-resume-partial-buffer.md):
that function resets its receive buffer on every call, so a budget that
returns early *discards* the frames it declined to read. Measured, a cap
of 4 or 16 frames does improve cadence (stalls 10 → 4/5, missed periods
1.79% → 0.59/0.85%) while inbound delivery collapses 282 → 10 msg/s and
chain delivery halves. That is a drop policy, not a budget.

The prerequisite is a resumable rx path. Until then the device-side half
is specified but not shippable, and the RFC does not pretend otherwise.

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
  unusable as the fix.
- **Demoting the transport band below the tiers.** This is the
  configuration d708d8c5b fixed; it starves the RX drain into
  multi-second RTO freezes.
- **A pure rate cap, no burst term.** Refuted by the cap-at-offered-rate
  cell above.

## Open questions

1. **Per-subscription declaration, per-session enforcement.** The budget
   is naturally per-session (one read task, one socket) while the
   declaration is per-subscription. The keyexpr *is* decoded before
   dispatch, so rx-side classification is cheap — but whether the budget
   can be attributed per-endpoint without a second buffer is unresolved.
2. **What validates `rate_hz` at resolve time?** Nothing records a
   per-board absorb capacity, and the one figure measured (~750 msg/s)
   turned out to describe burstiness rather than capacity.
3. **Does BEST_EFFORT already express part of this?** Unchecked: whether
   zenoh-pico's reliability setting causes the router to shed for a
   cooperative publisher. It cannot be the whole answer — the threat
   model includes a non-cooperating publisher — but it should be
   positioned relative to this RFC.
4. **Failure mode.** Reject at resolve time when declared ingress
   exceeds a known capacity, or admit and shed with a counter? Whatever
   sheds must count; silent shedding is how this class of problem hides.

## Changelog

- 2026-08 — initial draft, from the #0506 investigation (trace
  attribution, router pacing probe, drain budget probe).
