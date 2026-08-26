---
id: 823
title: "The runtime treats the QoS it REQUESTED as the QoS it got — six
  `*_get_actual_qos` slots are inert and nothing reads a downgrade"
status: resolved
type: bug
area: rmw
related: [phase-376, phase-393, issue-0800]
---

## Problem

QoS is a *negotiation*. A backend may grant less than asked: a DDS reader
requesting RELIABLE matches a BEST_EFFORT writer only if it downgrades, a
history depth larger than the middleware's limit is clamped, a deadline the
transport cannot meet is not honoured.

Upstream exposes what was actually granted through six symbols:

```
rmw_publisher_get_actual_qos
rmw_subscription_get_actual_qos
rmw_client_request_publisher_get_actual_qos
rmw_client_response_subscription_get_actual_qos
rmw_service_request_subscription_get_actual_qos
rmw_service_response_publisher_get_actual_qos
```

All six have vtable slots. All six are **inert** — no backend fills them and
nothing calls them (issue 0800's classification). So this runtime has exactly
one QoS: the one it asked for. It reports that as fact, and a downgrade is
invisible at every layer.

## Why this is a bug and not a missing feature

The other 28 inert slots are reserved shapes for capabilities nobody has needed.
This one is different: the runtime does not merely lack the reading, it
*asserts the wrong one*. `nros doctor`, the entity dumps and every diagnostic
that prints a QoS print the request. A user debugging "why is nothing arriving"
is shown the QoS they wrote, which is precisely the value that is not in
question.

The failure it hides is the common one. RELIABLE-vs-BEST_EFFORT mismatch is the
single most frequent reason a ROS 2 pub/sub pair does not communicate, and the
symptom — silence — is identical to a topic-name typo, a domain mismatch
(issue 0801) and a discovery failure (issue 0803). Those three have all been
run to ground this month; each cost hours precisely because nothing distinguished
them from the others.

## Scope

Cyclone is where this is answerable: `dds_get_qos` on the reader/writer returns
what the participant actually holds, and `qos.cpp` already converts our
`rmw_qos_t` the other direction, so the mapping exists and needs inverting.

zenoh-pico has no negotiation to read back (its QoS is per-message flags), so
NULL there is a correct answer, not a gap — which is the distinction this ABI's
nullity is for.

## Direction

1. Implement `publisher_get_actual_qos` / `subscription_get_actual_qos` on
   cyclonedds; the four client/service variants are the same call on the
   entities behind a client or service.
2. Have the runtime CACHE the granted QoS at creation, as it already caches
   `supports_in_place`, so a reader costs nothing.
3. Report a DOWNGRADE where a user sees it. A granted QoS that differs from the
   requested one is the diagnostic; equality is silence.
4. A test that requests RELIABLE against a BEST_EFFORT peer and asserts the
   read-back differs from the request — the assertion must fail against
   today's code, which returns the request.

## Resolved for cyclonedds, 2026-08-27

`read_entity_qos` in `qos.cpp` is the inverse of `make_dds_qos`, and
`publisher_get_actual_qos` / `subscription_get_actual_qos` are wired to it.
Slot classification moved **produced 33 -> 35, inert 34 -> 32**.

One deliberate choice: `out` arrives carrying the REQUESTED profile and any
field Cyclone does not report is left as it came in. An unreported field then
reads as "unchanged" rather than as a zero that looks like an answer — a zeroed
`depth` is indistinguishable from a real grant of 0.

`KEEP_ALL` is the case where that matters: Cyclone reports no meaningful depth
for it, so the requested depth stands rather than being overwritten with
garbage.

### The test refuses to pass vacuously

"The slot returns OK" would be satisfied by an implementation that echoes its
input — which IS this bug. So `actual_qos.cpp` pre-loads the out-struct with
values the entity cannot have (RELIABLE where BEST_EFFORT was requested,
`depth = 0xBEEF`, `deadline = 999999`) and asserts each field came back as the
entity's. An echoing implementation returns the sentinels, a zeroing one returns
zeros, and only a real `dds_get_qos` returns the granted values.

Verified: with `read_entity_qos` short-circuited to `return OK` — the exact bug
shape — ctest fails (exit 8); with the real read it passes.

### Not done here

* The four client/service variants. Same call on the entities behind a client or
  service; left for phase-393 W1 rather than done blind, since each needs the
  right entity handle and none has a consumer yet.
* Step 3 of the Direction — REPORTING a downgrade where a user sees it. The
  read-back now exists; nothing yet compares granted against requested and says
  so. That is the half that turns this from an API into a diagnostic, and it is
  what makes the RELIABLE/BEST_EFFORT silence distinguishable from a name typo.

zenoh-pico stays NULL, which is correct rather than missing: its QoS is
per-message flags with no negotiation to read back.
