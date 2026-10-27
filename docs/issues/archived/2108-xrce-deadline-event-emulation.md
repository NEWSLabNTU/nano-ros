---
id: 2108
title: "XRCE has no status events; deadline-missed events and next_deadline_ms are implementable client-side, the rest are not"
status: resolved
type: enhancement
area: [rmw, xrce, events]
severity: low
found: 2026-10-27
resolved: 2026-10-27
related: [1329, 1978]
---

## What is missing

XRCE leaves `subscription_event_init`, `publisher_event_init`, both
`*_take_event`, `publisher_assert_liveliness`, `event_type_is_supported` and
`next_deadline_ms` NULL. `docs/reference/xrce-known-limitations.md` ("Deadline
events absent") records that the Rust implementation this C backend replaced (phase 115.K.2) emulated
`OfferedDeadlineMissed` / `RequestedDeadlineMissed` from the platform clock and
the C backend never ported it.

## The split

- **Protocol cannot:** liveliness changed/lost, incompatible QoS, matched
  events and manual liveliness assertion. DDS status lives on the Agent's
  entities and the XRCE protocol carries no status submessage to the client
  (`uxrOnStatusFunc` carries a per-REQUEST status for an operation the client issued, nothing about DDS entity state). Stock
  `rmw_microxrcedds` answers `rmw_*_event_init` with UNSUPPORTED.
- **Implementable here:** requested/offered DEADLINE. Both are local facts —
  time since the last take-able arrival (`xrce_topic_callback`) or the last
  `publish` — and `next_deadline_ms` is the earliest such expiry, which is what
  the executor's wait bound reads.

## Precondition

DEADLINE is not in `xrce_supported_qos_policies` today, so a profile asking for
one is refused at create. Emulating the event is what EARNS that bit
(`rmw_vtable.h`: "a bit is earned, not declared"), and `check-qos-mask-derivation`
must agree. `event_type_is_supported` then answers true for the two deadline
kinds only.

## Acceptance

A no-Agent test that a subscription with a 50 ms deadline and no arrivals
reports `RequestedDeadlineMissed` with the right `total_count_change`, and that
`next_deadline_ms` names the expiry.

## Resolution (2026-10-27)

Both DEADLINE events are emulated client-side (`src/session.c`,
`xrce_deadline_*`).

### How it works

- **Tracker.** Each subscription slot and each publisher state carries an
  `xrce_deadline`.
- **Arming.** `subscription_event_init` / `publisher_event_init` arm it for
  `REQUESTED_` / `OFFERED_DEADLINE_MISSED`. The period is the registration's
  `deadline_ms`, else the profile's `deadline_ms`; 0 and infinite arm nothing.
  Every other kind answers UNSUPPORTED.
- **Restart.** A takeable arrival (`xrce_topic_callback`) restarts the
  subscription's period; a publish (`publish_raw` / `publish_streamed`)
  restarts the publisher's.
- **Counting.** At the end of every `drive_io`, on the executor's thread,
  every whole period that ended is one miss: `total_count` grows by all of
  them, and the callback fires once per check with the accumulated
  `total_count_change`. DDS counts periods, not checks.
- **Executor wake-up.** `next_deadline_ms` reports the earliest armed expiry,
  so a quiet executor wakes for it.
- **Kind probe.** `event_type_is_supported` is true for the two DEADLINE
  kinds only.
- **QoS bit.** This earned `NROS_RMW_QOS_POLICY_DEADLINE`. The
  `nros-qos-honours: DEADLINE` claims sit where publisher.c / subscriber.c
  read `qos->deadline_ms`, and `check-qos-mask-derivation` agrees. Its
  issue-1329 self-test mutation now uses LIFESPAN, since DEADLINE is no longer
  an over-claim.
- **Unchanged:** `*_take_event` and manual liveliness stay NULL.

### Bound

The Agent's DDS entity still offers / requests an infinite deadline, because
`uxrQoS_t` cannot carry one. A stock subscriber requesting a finite deadline
therefore does not match an XRCE publisher, and the read-back slots report the
entity's INFINITE deadline. The event is this client's view only. This is
stated in `xrce-known-limitations.md` and §8 of `rmw-vs-upstream.md`.

### Tests

- **No Agent:** `tests/deadline.c` (`nros_rmw_xrce_deadline`,
  `just check rmw-xrce`). Time is driven, never slept. Checks:
  - with a 50 ms deadline, no miss at 49 ms;
  - one miss at 50 ms (total 1, change 1, the registering context);
  - a check at 260 ms counts total 5, change 4, in ONE call;
  - `next_deadline_ms` names the expiry;
  - an arrival restarts the period;
  - NULL callback / infinite deadline disarm, and `deadline_ms` 0 falls back
    to the profile;
  - a publisher's offered deadline is restarted by a publish and unlisted at
    destroy;
  - `event_type_is_supported` and the mask bit.
  - Negative control: counting one miss per check and dropping the arrival
    restart failed four checks.
- **Live:**
  `xrce_deadline::xrce_subscription_reports_requested_deadline_misses`
  (feature `xrce-wake-test`, `just check required-features-tests` /
  test-all) uses a real Agent and a 100 ms deadline.
  - ~550 ms with no publisher reported total 5 in 5 callbacks.
  - A same-session publisher every 20 ms, samples taken, added no miss.
  - Negative controls: without the arrival restart the traffic phase went from
    5 to 11 misses; with the event slot NULL the registration answered
    `Unsupported`.
