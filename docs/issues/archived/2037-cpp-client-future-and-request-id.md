---
id: 2037
title: "C++ client futures lack rclcpp's FutureAndRequestId surface: no .future, no .request_id, no share()"
status: resolved
type: enhancement
area: [api]
severity: low
found: 2026-10-25
resolved: 2026-10-25
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

## Resolution

Resolved 2026-10-25. On a hosted build, `PollClient<S>::async_send_request(shared_ptr)`
returns `PollClient<S>::FutureAndRequestId`, a struct with two public members:

- `.future`, an `rclcpp::Future<Response>`;
- `.request_id`, the sequence id the reply is matched on (issue 2009).

C++17 structured bindings work on it. `get()`, `wait_for()`, `valid()` and
`is_ready()` forward to the future, so `spin_until_future_complete` and the
stock `minimal_client` are unchanged.

- `share()` moves the future into heap shared state and returns a
  `SharedFutureAndRequestId`. Its `future` is a
  `rclcpp::detail::DrivenSharedFuture<SharedResponse>`, which IS-A
  `std::shared_future`. Every copy gets the same response object.
- A request that is removed, or whose client goes away, settles to a null
  response instead of waiting.
- `DrivenSharedFuture` and `PumpCell` moved from `rclcpp_action/detail/upstream.hpp`
  to `rclcpp/detail/driven_future.hpp`. The action header keeps its names as
  aliases. A new hosted-family id, `shared-future-interop`, covers them.
- `remove_pending_request` now also takes `int64_t request_id`
  (`PendingList::remove_seq`), `FutureAndRequestId&` and
  `const SharedFutureAndRequestId&`.
- Without `std::promise`, `share()` is a `static_assert`
  (`NROS_RCLCPP_REFUSE_SHARED_FUTURE`).

Held by `client_future_and_request_id_runtime.cpp`, which runs on the stub
backend and stages replies by request id. The freestanding refusal is held by
`client_share_freestanding_probe.cpp`. Both run in `just check cpp`.
Negative control, measured: making `request_id` always `SEQUENCE_ID_UNKNOWN`
fails the test, and so does pumping the shared state without fulfilling the
promise.

## Still open

- `.future` is an `rclcpp::Future`, not a `std::future`. Nothing fulfils a
  promise from another thread here (RFC-0021). It has the members a ported
  caller uses.
- The verb lives on `PollClient`, not on `rclcpp::Client` (the dispatch half).
- The callback overload `async_send_request(request, callback)` and
  `SharedFutureWithRequestAndRequestId` are absent.
