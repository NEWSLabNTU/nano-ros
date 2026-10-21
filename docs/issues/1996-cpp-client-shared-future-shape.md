---
id: 1996
title: "A ported rclcpp service client still needs ~15 edited lines: `async_send_request(shared_ptr)`, `future.get()` and `remove_pending_request` have no counterpart"
status: open
type: limitation
severity: low
area: [api]
related: [phase-488, 0339]
found: 2026-10-21
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
