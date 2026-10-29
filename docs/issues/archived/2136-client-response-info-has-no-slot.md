---
id: 2136
title: "A service client receives no response info: the ABI has no take_response_with_info slot, so the reply's timestamps and the server's identity never reach it"
status: resolved
type: limitation
area: [rmw, abi, services]
severity: low
found: 2026-10-29
related: [2135, 2081, 1942, 0778, 2148]
resolved: 2026-10-29
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

## Resolution

Resolved 2026-10-29.

- **ABI.** `nros_rmw_vtable_t` gains the optional, appended slot
  `take_response_with_info(client, reply, seq_out, taken,
  rmw_service_info_t *)` (`rmw_vtable.h`), the twin of
  `take_request_with_info`. Bindings regenerated; `rmw-abi-shape` records it
  as ADDED; the RFC-0035 slot table is regenerated (77 slots).
- **Runtime.** `ClientTrait::response_info()` (default `NOT_REPORTED`).
  `CffiClient::take_response_raw` dispatches the slot when non-NULL and keeps
  the struct (zeros → `None`); the Rust adapter fills the slot from a Rust
  backend's `response_info()`, so no side table is needed.
- **Backends.**
  - zenoh: the reply callback now stores the reply's attachment and a
    wall-clock receive stamp (`zpico_get_check_info`), so `request_id` (the
    client's gid and sequence number) and `source_timestamp` come from the
    attachment and `received_timestamp` from the stamp. Our server, which
    echoes the request attachment, now restamps its timestamp with the
    server's send time, as `rmw_zenoh_cpp` writes it.
  - Cyclone DDS: the reply's echoed request header + publication handle and
    its source timestamp, by the same rule as a request (stock
    `rmw_cyclonedds_cpp` shares one take). No received timestamp in 0.10.
  - XRCE-DDS: the reply callback stamps `received_timestamp`; the request id
    carries the sequence number only (the Agent owns the request writer, so no
    GUID), which the runtime reports as no request id.
- **Surfaces.** Rust: `Promise::take_with_info` / `wait_with_info`. C:
  `rclc_executor_add_client_with_request_id` (rclc's shape; the sequence
  number always, the writer GUID where reported). C++ is not surfaced yet —
  issue 2148, with the Rust callback-client form.
- **Tests.** cffi dispatch / zero-struct / no-slot unit tests and an adapter
  seam test; zenoh unit tests and the router integration test (reply id,
  server send stamp inside the server's send window, receive stamp after it);
  XRCE `received_timestamp.c` reply section; Cyclone `service_roundtrip.cpp`;
  nros-node `promise_take_with_info_reports_the_replys_info`; C
  `typed_service_client.c` step 5; live `interop_e2e`
  `case_11_zenoh_service_response_info` against a stock `rmw_zenoh_cpp`
  server. Negative controls (each measured failing, then reverted): the cffi
  conversion dropped, the zenoh server restamp disabled, the XRCE stamp
  removed, the Cyclone fill removed, the C request id built without the
  reported GUID.

