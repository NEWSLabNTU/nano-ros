---
id: 2109
title: "XRCE leaves the three per-entity new-data callback slots NULL, though its topic/request/reply callbacks already name the entity"
status: resolved
type: enhancement
area: [rmw, xrce, executor]
severity: low
found: 2026-10-27
resolved: 2026-10-27
related: [1940]
---

## What is missing

`subscription_set_on_new_message_callback`, `service_set_on_new_request_callback`
and `client_set_on_new_response_callback` are NULL for XRCE, so every
per-entity wake in the C, C++ and async Rust APIs (issue 1940) is inert on this
backend and those entities fall back to polling `has_data` / `has_request`.

## Why it is implementable

`set_wake_callback` is correctly NULL (phase 124.B.1): micro-XRCE-DDS-Client
has no background thread, so nothing arrives outside `drive_io`. But the
per-entity slots ask a different question — WHICH entity got data — and the
client answers that on every arrival: `uxrOnTopicFunc` / `uxrOnRequestFunc` /
`uxrOnReplyFunc` carry the `uxrObjectId`, and this backend already maps it to a
slot (`xrce_topic_callback`). Firing the registered callback from there, on the
thread running `drive_io`, is exactly the slot's contract ("a backend may call
once per arrival with 1"). Stock `rmw_microxrcedds` answers these with
UNSUPPORTED, so this would be ahead of upstream.

## Acceptance

A no-Agent test (inject an arrival through the topic callback) that the
registered callback fires once with the registering entity's user data, and a
registration that is replaced or cleared stops firing.

## Resolution (2026-10-27)

All three slots are filled.

- **Storage.** Each slot struct (subscriber, service server, service client)
  keeps a `(callback, user_data)` pair.
- **Setters.** `xrce_subscription_set_on_new_message_callback`,
  `xrce_service_set_on_new_request_callback` and
  `xrce_client_set_on_new_response_callback` write the pair under the session
  lock.
- **Firing.** The arrival callbacks (`xrce_topic_callback`,
  `xrce_request_callback`, `xrce_reply_callback`) fire it once with 1 when an
  arrival becomes takeable. They run inside `drive_io`, which holds the same
  lock. That is what makes "after a replace or clear returns, the previous pair
  never runs" true.
  - An arrival dropped because the ring is full fires nothing: nothing became
    takeable.
  - An oversized arrival does fire: its take reports `MESSAGE_TOO_LARGE`, and
    that is still something to take.
- **Destroy.** Destroying the entity clears the pair, so a reused slot fires
  nothing stale.
- `set_wake_callback` stays NULL. Nothing arrives outside `drive_io`.

### Tests

- **No Agent:** `tests/new_data_callbacks.c` (`nros_rmw_xrce_new_data_callbacks`,
  `just check rmw-xrce`) injects arrivals through the three callbacks and
  checks:
  - each fires once, with 1 and the registering user data;
  - another entity's arrival does not fire it;
  - a dropped arrival (ring full) fires nothing;
  - replace and clear take effect;
  - destroy clears the registration;
  - NULL entities return INVALID_ARGUMENT.
  - Negative control: removing the reply-path fire and the destroy-path clear
    failed four checks.
- **Live:** `xrce_entity_wake::each_xrce_arrival_marks_its_own_subscription`
  (feature `xrce-wake-test`, lane `just check required-features-tests` /
  `test-all`) uses a real Agent. Two subscriptions on one executor each arm an
  attributed slot. A sample published on each topic comes back through the
  Agent and marks exactly its own subscription's ready bit. Negative controls:
  - Removing the subscription slot from the vtable left `armed 0x0` (the
    executor attributed nothing).
  - Removing the fire in `xrce_topic_callback` gave "no entity wake within
    10s".
