---
id: 2148
title: "A C++ service client gets no response info: the rclcpp client has no take_response family to carry a request header"
status: resolved
type: limitation
area: [api, cpp, services]
severity: low
found: 2026-10-29
related: [2136, 2081, 2037]
resolved: 2026-10-30
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

## Resolution

Resolved 2026-10-30.

- **C++.** `rclcpp::PollClient<S>::take_response(response, request_header)`
  (+ `take_response_sized<Cap>`), upstream `rclcpp::Client::take_response`:
  the next reply with its `rmw_request_id_t`, through the new FFI
  `nros_cpp_service_client_take_response_with_request_id` (excluded from
  cbindgen, declared against `rmw_request_id_t`, which moved to the one
  header `rclcpp/detail/rmw_request_id.hpp` shared with the service side).
  The sequence number always, the writer GUID where the backend reports it,
  zero otherwise (`RawRequestId::for_response`, C's rule). As upstream's it
  takes whatever reply is next and touches no pending future. Returns
  `Result` (`TryAgain` when nothing is waiting) where upstream returns `bool`
  and throws. On the poll client only: the dispatch `rclcpp::Client<S>`'s
  arena takes every reply for its handler, and upstream has no
  handler-with-header shape to adopt there.
- **Rust.** `Node::create_client_with_callback_with_info`, closure
  `FnMut(&Reply, &ServiceInfo)`. The typed callback entry now dispatches
  through `ClientReplyCallback` (`ReplyOnly` / `ReplyWithInfo`); the info is
  read from `ClientTrait::response_info` right after the matched take, as
  `Promise::take_with_info` reads it.
- **Tests.** nros-node `callback_client_with_info_reports_each_replys_info`
  (mock; control: the closure handed `NOT_REPORTED` -> fails). C++
  `client_take_response_runtime.cpp` in `just check cpp` (stub; control: the
  FFI writing `RawRequestId::default()` -> 5 failures). Live:
  `interop_e2e::interop::case_12_zenoh_service_response_header_cpp` on the new
  `native-service-cpp-zenoh-n2r` cell — a stock rclpy `rmw_zenoh_cpp` server
  answers `bins/service-info-client-cpp` (fixture row
  `service-info-client-cpp-zenoh`); asserted over three calls: one non-zero
  GUID, header sequence numbers 1..=3 in order, sums correct.
  Measured (ros2 distrobox, Humble):
  `guid=95f3dd570b71bdbe015f3e39f888c6c5 seq=1..3 sum=10..12`. Negative
  control: the FFI writing `RawRequestId::default()` prints
  `RESPONSE_INFO ABSENT … seq=0` for every reply and fails the case
  (measured, reverted).
- **Docs.** rmw-vs-upstream "Service info" client surfaces; rmw-api-map's
  `rmw_service_info_t` rule (comparison regenerated); ledger rows
  `cpp:PollClient::take_response{,_sized}`,
  `rust:Node::create_client_with_callback_with_info`, and
  `cpp:Client::take_response` amended.
