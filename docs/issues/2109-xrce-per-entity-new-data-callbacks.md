---
id: 2109
title: "XRCE leaves the three per-entity new-data callback slots NULL, though its topic/request/reply callbacks already name the entity"
status: open
type: enhancement
area: [rmw, xrce, executor]
severity: low
found: 2026-10-27
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
