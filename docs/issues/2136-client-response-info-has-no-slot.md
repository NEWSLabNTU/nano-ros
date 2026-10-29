---
id: 2136
title: "A service client receives no response info: the ABI has no take_response_with_info slot, so the reply's timestamps and the server's identity never reach it"
status: open
type: limitation
area: [rmw, abi, services]
severity: low
found: 2026-10-29
related: [2135, 2081, 1942, 0778]
---

## What

Upstream `rmw_take_response` fills an `rmw_service_info_t` beside the reply:
the request id it answers, plus `source_timestamp` and `received_timestamp`.
`rmw_zenoh_cpp` stamps the reply's receive time
(`ZenohReply::get_received_timestamp`), and Jazzy `rmw_cyclonedds_cpp` stamps
it at take time.

Here, `take_response` returns the payload and `int64_t *seq_out` only. That
is the `sequence_id` `send_request` handed out, which the runtime matches a
reply against (issue 0778). There is no `take_response_with_info` slot, the
way issue 2081 added `take_request_with_info` on the server side, so:

- no backend can report a reply's timestamps or the replying server's
  identity, and
- no client API (Rust `Client`, rclc, rclcpp) surfaces them.

`docs/reference/rmw-api-map.toml` records this as the `rmw_take_response`
arg rule. Issue 2135 filled `received_timestamp` on subscriptions and on
service REQUESTS only, and stopped here because this needs new ABI.

## What would close it

- An appended optional vtable slot `take_response_with_info(client, span,
  seq_out, taken, rmw_service_info_t*)`, the twin of
  `take_request_with_info`.
- The runtime dispatching through it.
- A client-side API that hands the info to the caller.

Backends could then fill:

| backend | `received_timestamp` |
| --- | --- |
| zenoh | stamp in the reply callback |
| XRCE | stamp in `xrce_reply_callback` |
| Cyclone | none (0.10 has no reception time) |

`source_timestamp` is reported where the reply carries one: zenoh's reply
attachment and Cyclone's sample info.
