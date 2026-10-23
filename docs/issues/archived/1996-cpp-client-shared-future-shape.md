---
id: 1996
title: "A ported rclcpp service client still needs ~15 edited lines: `async_send_request(shared_ptr)`, `future.get()` and `remove_pending_request` have no counterpart"
status: resolved
type: limitation
severity: low
area: [api]
related: [phase-488, 0339]
found: 2026-10-21
resolved: 2026-10-23
---

## What happens

The stock `ros2/examples` `minimal_client` writes

```cpp
auto request = std::make_shared<AddTwoInts::Request>();
auto result_future = client->async_send_request(request);
if (rclcpp::spin_until_future_complete(node, result_future) != rclcpp::FutureReturnCode::SUCCESS) {
  client->remove_pending_request(result_future);
  return 1;
}
auto result = result_future.get();
... result->sum ...
```

After phase-488 (`Node::make_shared`, a chrono `wait_for_service`, and a
`spin_until_future_complete` that finally accepts a real `rclcpp::Future`),
the port still rewrites 15 lines (`edits.json`, `minimal_client`):

- `async_send_request(std::shared_ptr<Request>)` → `send_request(const Request&)`;
- `future.get()` returning a `shared_ptr<Response>` → `Future::try_take(Response&)`;
- `remove_pending_request(future)` → `Future::cancel()` (the
  `cpp:Client::remove_pending_request` row is `declined`).

## What would close it, and what it costs

- `PollClient::async_send_request(const std::shared_ptr<Request>&)` (hosted)
  forwarding to `send_request(*req)` costs nothing on its own.
- `Future::get()` has no allocation-free upstream shape: upstream returns an
  owning `shared_ptr<Response>`. A value-returning `get()` behind an
  `operator->` wrapper, or a hosted `make_shared` copy, are the options; both
  need a ruling on what `get()` does before the reply arrives (upstream
  blocks; RFC-0021 forbids a wait that does not drive the executor).
- `remove_pending_request` needs the sequence-number correlation the
  `cpp:Client::async_send_request` row records as missing.

## Acceptance

`minimal_client`'s count in `edits.json` drops, each remaining edit named by
a ledger row.

## Resolution

Resolved 2026-10-23. `minimal_client` is now **0** edited lines (`edits.json`,
was 15); the port is byte-identical to upstream and compiles, links and boots
in `just check cpp`.

- `rclcpp::PollClient<S>::async_send_request(const std::shared_ptr<Request>&)`
  (hosted, `NROS_CPP_HAS_SHARED_PTR`) forwards to `send_request(*req)`, as
  proposed above. A null request returns an empty future and sends nothing.
- `rclcpp::Future::get()` — the ruling the issue asked for. Hosted: it returns
  an owning `std::shared_ptr<T>` (one `make_shared` copy, consumes the future)
  **if the reply has arrived**, and otherwise NEVER waits: it says
  `NROS_RCLCPP_REFUSE_FUTURE_GET_NOT_READY_RUNTIME` and returns nullptr without
  consuming. Blocking was ruled out by RFC-0021 (a future has no executor of its
  own to drive), and the stock code only calls `get()` after
  `spin_until_future_complete` returned SUCCESS, so the non-blocking form ports
  it unchanged. Freestanding: `get()` is a `static_assert` carrying
  `NROS_RCLCPP_REFUSE_FUTURE_GET`, naming `try_take` / `wait`.
- `rclcpp::Future::wait_for(std::chrono::duration)` (hosted, new capability
  `NROS_CPP_HAS_STD_FUTURE` = `<future>` for `std::future_status` only) returns
  `ready`/`timeout` and DRIVES the client's executor for the budget in <= 10 ms
  `spin_once` slices; zero/negative is one check, as for `std::future`. The
  future now carries the executor handle of the client that made it (service
  poll client and action client).
- `rclcpp::Future::valid()` (both builds).
- `remove_pending_request(Future&)` on the poll client, both builds: cancels
  the future and answers true only for a still-pending request of THIS client.
  `cpp:Client::remove_pending_request` re-verdicted declined ->
  divergence/adopt-bounded on the previous verdict's own argument (removal by
  key over one candidate is the same operation as cancelling it).
- The stub backend (`packages/api/nros-c/tests/run/stub_rmw_backend.c`) gained
  `send_request`/`take_response` with a staged reply, so the runtime test
  (`client_future_shape_runtime.cpp`) drives a real `rclcpp::Future` through
  `libnros_cpp`; `client_future_get_freestanding_probe.cpp` is the
  expected-failure half. Both are in `just check cpp`.

## Still open

Sequence-number correlation. The future is still not keyed by the request's
sequence number (`cpp:PollClient::send_request`, slot 0; disposition ADOPT on
`cpp:Client::async_send_request`), so two requests in flight on one client
cross-deliver, and a reply that arrives after `remove_pending_request` is taken
by the client's next future. The vtable already carries the id
(`send_request`'s `sequence_id`, `take_response`'s `seq_out`); the nros-cpp FFI
drops it at three layers. There is also no `.future` / `.request_id` /
`share()` (no `std::shared_future`).
