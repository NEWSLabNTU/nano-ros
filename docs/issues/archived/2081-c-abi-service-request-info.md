---
id: 2081
title: "C-ABI RMW backends (Cyclone, XRCE) report no per-request `ServiceInfo`: the `take_request` slot has nowhere to put it"
status: resolved
type: limitation
area: [rmw, api]
severity: low
found: 2026-10-26
resolved: 2026-10-26
related: [issue-1942, phase-515, rfc-0054, issue-2088, issue-2089]
---

**Home: [phase-515](../../roadmap/phase-515-api-stock-shape-review.md)** (item R7) — the residue of issue 1942.

## What

Issue 1942 shipped `nros::ServiceInfo` (rclrs's per-request metadata: client GID +
client sequence number, source/received timestamps) and the seam method
`ServiceTrait::request_info`. A Rust backend's info crosses the C ABI through the
Rust adapter's side table, so zenoh reports `request_id` and `source_timestamp`.

A backend written against the C vtable reports nothing: every field is `None`.
The `take_request` slot (`packages/core/nros-rmw-abi/include/nros/rmw_vtable.h`)
returns only `seq_out`, and its doc says that token "stands in for
`rmw_service_info_t *` — an RTOS reply needs the sequence and nothing else in that
struct". Cyclone has the facts (DDS sample info: source timestamp, the request
header's writer GUID and sequence number) and is never asked for them.

## What a fix needs

A `take_request_with_info` vtable slot (or a nullable info out-parameter), the
service twin of `take_with_info`: header edit, `scripts/gen-abi-bindings.sh`,
`rmw-api-parity` classification, `CffiService::take_request` dispatching to it when
non-NULL (falling back to the side table, as `take_serialized_with_info` does),
and Cyclone's `vtable.cpp` filling it. The ledger row
`c:take_request_with_info` (`declined` because "the ABI DECLARES the omission")
would then reopen — its own text says to re-open it if the ABI grows the pair.

## Acceptance

A Cyclone service server's `ServiceInfo` carries the requester's writer GUID and
sequence number, measured against a stock `rmw_cyclonedds_cpp` client.

## Resolution (2026-10-26)

**ABI.** `nros/rmw_entity.h` gains upstream's `rmw_request_id_t` (16-byte
`writer_guid` + `sequence_number`) and `rmw_service_info_t` (source and
received timestamps + the request id), field for field. Zero means "not
reported", as in `rmw_message_info_t`. The vtable gains an optional slot,
APPENDED at the end so existing backends keep their layout:

```c
rmw_ret_t (*take_request_with_info)(const rmw_service_t *server,
    rmw_mut_byte_span_t *request, int64_t *seq_out, bool *taken,
    rmw_service_info_t *service_info);
```

`seq_out` stays beside the struct, not inside it. A backend's reply token (a
reply-slot index on Cyclone, XRCE and zenoh) is not the client's sequence
number, and the runtime needs both. Bindings regenerated with
`scripts/gen-abi-bindings.sh`. `rmw-abi-shape` records the slot as ADDED,
pinned.

**Runtime.** `CffiService::take_request` dispatches the slot when it is
non-NULL and converts the struct (`service_info_from_raw`): a zero timestamp
becomes `None`, and an all-zero GUID means no request id. A NULL slot falls
back to `take_request` plus issue 1942's side table, so Rust backends (zenoh)
are unchanged.

**Backends.**

| backend | `request_id` | `source_timestamp` | `received_timestamp` |
| --- | --- | --- | --- |
| zenoh (unchanged, issue 1942) | query attachment | query attachment | — |
| Cyclone DDS | request-header guid (8 B LE) + publication handle (8 B LE), header seq | DDS sample source timestamp | — (Cyclone 0.10 has none; upstream writes 0 too) |
| XRCE-DDS | the Agent's `SampleIdentity`: 12-byte prefix + entity id, `(high << 32) \| low` | — | — |

Cyclone fills the struct the way stock `rmw_cyclonedds_cpp`'s
`rmw_take_response_request` does, and XRCE the way `rmw_microxrcedds` does, so
a server here reports what a stock server would for the same request.

**C and C++ callbacks.** `nros_node::RawRequestId` / `RawServiceRequestIdCallback`
and `Executor::register_service_raw_with_request_id_sized` give a raw service
callback the id of the request being dispatched. C uses
`rclc_executor_add_service_raw_with_request_id(executor, service, cb, ctx)`.
Its callback, `rclc_service_raw_callback_with_request_id_t`, takes
`(request, request_len, rmw_request_id_t *request_id, response, cap, len, ctx)`,
rclc's argument order in CDR-byte form. In C++, rclcpp's three-argument
handler `(std::shared_ptr<rmw_request_id_t>, request, response)` now gets the
id in its request header (it was always zero), for both function-pointer and
stateful handlers. `rmw_request_id_t` is declared under one include guard in
the ABI header, `<rclc/detail/request_id.h>` and rclcpp's `service.hpp`.

**Tests.**

- nros-rmw-cffi unit tests: the slot is dispatched instead of `take_request`,
  every field is converted, the reply token is kept separate from the client
  sequence, and an all-zero struct reads as `NOT_REPORTED`.
- `service_request_id.c` (`just check c`) and
  `service_request_header_runtime.cpp` (`just check cpp`) run on the stub
  backend, which stages a request with an id: the id reaches the callback, a
  request staged without one arrives zeroed, and a backend with no slot still
  dispatches.

**Acceptance, measured** (2026-10-26, ros2 distrobox, Humble): in
`ros2_srv_e2e.sh`, a stock `rmw_cyclonedds_cpp` client (`ros2 service call
/add_two_ints`) calls the nano-ros Cyclone server. The server now takes the
request through `take_request_with_info` and logs
`REQUEST_ID guid=97a16f924367db54bf44574a536d7b13 seq=1 source_ts=1793028907113219509 received_ts=0`.
The e2e asserts a non-zero GUID, `seq >= 1`, a source timestamp and
`received_ts=0`. In-process, `service_roundtrip.cpp` asserts that the
reported sequence number equals the one `send_request` gave the nano-ros
client. Both run in `just check rmw-cyclonedds`.

**XRCE is compiled, not measured live.** `just check rmw-xrce` builds the
slot. No run against an Agent has checked the reported `SampleIdentity` yet.
That is issue 2088. The typed C service path's missing request-id flavour is
issue 2089.

Ledger: `c:take_request_with_info` stays `declined`, with the reason narrowed:
the ABI now has the pair, but the rcl polling family it belongs to is not
offered. `c:executor_add_service_with_request_id` names the new entry point.
`cpp:Node::create_service` records the filled header.
