---
id: 2037
title: "C++ client futures lack rclcpp's FutureAndRequestId surface: no .future, no .request_id, no share()"
status: open
type: enhancement
area: [api]
severity: low
found: 2026-10-25
related: [1996, 2009, phase-488]
---

## What

Upstream `rclcpp::Client<S>::async_send_request` returns a
`Client::FutureAndRequestId`. It is a `std::future` wrapper with a public
`.future` member and a `.request_id` (the `int64_t` sequence number), and
`share()` turns it into a `SharedFutureAndRequestId`. Callers use the id with
`remove_pending_request(request_id)` and use the shared form to hand one reply
to several waiters.

Ours returns `rclcpp::Future`. Issue 1996 gave it `get()`, `wait_for()` and
`valid()`. Issue 2009 keyed it by the request's sequence id internally
(`rclcpp/future.hpp`, `PendingList`). The id is not exposed, though, and
neither is the member layout:

- `auto [future, id] = ...` and `f.request_id` do not compile;
- `f.future.get()` does not compile;
- `f.share()` does not compile.

The ledger records this only as a sentence in `cpp:Client::async_send_request`
("there is no `.future` / `.request_id` / `share()`"). It has no row of its own,
because the extractor records `Client::FutureAndRequestId` and
`SharedFutureAndRequestId` only as return types
(`docs/reference/api-surface/jazzy/rclcpp.json`), not as types with members.

## Notes

- `.request_id` is cheap: the future already holds its id (issue 2009).
- `.future` as a real `std::future` needs a promise someone fulfils, and
  nothing fulfils from another thread here (RFC-0021). The likely shape is the
  `DrivenSharedFuture` pattern issue 2027 used for action futures.
- `share()` is hosted-only for the same reason.

## Done when

A hosted C++ build compiles `.request_id`, `.future` and `share()` on the
value `async_send_request` returns, with a runtime test on the stub backend,
and the `cpp:Client::async_send_request` row names what is left.
