---
id: 2108
title: "XRCE has no status events; deadline-missed events and next_deadline_ms are implementable client-side, the rest are not"
status: open
type: enhancement
area: [rmw, xrce, events]
severity: low
found: 2026-10-27
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
