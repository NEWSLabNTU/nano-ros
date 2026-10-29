---
id: 2148
title: "A C++ service client gets no response info: the rclcpp client has no take_response family to carry a request header"
status: open
type: limitation
area: [api, cpp, services]
severity: low
found: 2026-10-29
related: [2136, 2081, 2037]
---

## What

Issue 2136 gave a service client upstream `rmw_take_response`'s
`rmw_service_info_t`: the vtable's `take_response_with_info` slot, filled by
zenoh, Cyclone DDS and XRCE-DDS, kept by the runtime as
`ClientTrait::response_info`. Rust reads it through
`Promise::take_with_info` / `wait_with_info`, and C through
`rclc_executor_add_client_with_request_id` (the request id only).

C++ reads none of it. Stock rclcpp hands a client's request header out only
through `Client::take_response(response, rmw_request_id_t & request_header)`
(and its type-erased twin), and our clients have no `take_response` family at
all: a reply arrives through an `rclcpp::Future` (`PollClient`) or a response
callback (`Client`), neither of which carries a header
(`docs/reference/api-parity-ledger/service/cpp.Client..take_response.json`).

The Rust callback client (`Node::create_client_with_callback`, a
`FnMut(&Reply)` closure) has the same gap: only the `Promise` path carries
the info.

## What would close it

- An FFI read of the handle's `response_info` after a take
  (`nros_cpp_service_client_take_response_seq` already runs the take), and a
  C++ surface for it — `take_response(Response &, rmw_request_id_t &)` on the
  poll client, the stock shape, or the header beside the future's response.
- A `create_client_with_callback` form whose closure receives
  `&ServiceInfo`, matching `create_service_with_info`.
