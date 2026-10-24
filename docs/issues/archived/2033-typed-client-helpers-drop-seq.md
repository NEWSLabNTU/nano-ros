---
id: 2033
title: "The typed ClientTrait helpers drop the sequence id the raw path returns, so a typed caller cannot tell a stale reply from its own"
status: resolved
type: bug
severity: low
area: [api, rmw]
found: 2026-10-24
resolved: 2026-10-24
related: [2009, 0778]
---

## What happened

Issue 2009 threaded the request id through every client surface that takes a
reply — except the typed helpers on the RMW trait itself.
`nros_rmw::ClientTrait::send_request::<S>` serialized the request, called
`send_request_raw` and threw its id away (`.map(|_seq| ())`), and
`take_response::<S>` matched `Some((len, _seq))` and returned the reply alone,
documented as "the reply to the most recently sent request" — which it never
checked. A typed caller that resent after a timeout had no id to compare, so
the abandoned request's late reply was decoded and returned as the resend's.
No in-tree code called the two helpers; out-of-tree users of the trait did.

The same shape sat in three in-tree C++ examples, one layer up: the workspace
`AddClient` components (`examples/workspaces/{cpp,mixed}`) and the Zephyr
`AddTwoIntsClient` sent with `nros_cpp_service_client_send_request` and polled
with `nros_cpp_service_client_take_response`, the two FFI spellings that report
no id. The workspace clients RESEND after six empty ticks, so a slow first
reply printed the wrong sum against the second request.

Sweep: `git grep -nE "take_response_raw|send_request_raw|_take_response\(|_send_request\(" -- '*.rs' '*.cpp' '*.hpp' '*.c'`
over `packages/` and `examples/`. Every other typed wrapper already correlates:
the Rust `Promise`, the executor callback clients and `nros` `call_raw` (2009),
`rclcpp::Future` / `PollClient`, the C++ polling action client's `_raw_for`
takes, and the C generated `<Svc>__client_{send_request,take_response}`
(through `rclc_client_*`, which keep the call's ids in the arena entry).

## Resolution

- `ClientTrait::send_request::<S>` returns the id (`Result<i64, _>`).
- `ClientTrait::take_response::<S>` returns `Option<(S::Reply, i64)>` — the
  reply and the id of the request it answers, upstream's `rcl_take_response`
  header.
- New `ClientTrait::take_response_for::<S>(sequence_id, buf)`: the typed take
  for ONE request. It applies `nros_rmw::correlation`'s rule through a one-entry
  `RequestCorrelator` and `take_correlated`, so a stale reply is dropped (at
  most `MAX_STALE_DRAIN_PER_POLL` per call) and an id-less reply or request
  (`SEQUENCE_ID_UNKNOWN`) matches the request in mind.
- The three C++ examples use `nros_cpp_service_client_{send_request,take_response}_seq`
  and drop a reply naming another request. `ServiceClientStorage`'s doc points
  at the `_seq` pair.

Tests: `nros_rmw::correlation` unit tests
`typed_take_for_drops_the_late_reply_of_an_abandoned_request` (send, resend,
the first request's reply arrives first: the resend gets only its own),
`typed_take_for_reports_nothing_when_only_a_stale_reply_arrived`,
`typed_take_for_accepts_an_idless_reply_for_the_request_in_mind`,
`typed_send_returns_the_id_the_backend_assigned`,
`typed_take_reports_the_id_of_the_request_a_reply_answers`. Mutation: making
`take_response_for` take the next reply regardless of id fails the first two.

## Still open

- The id-less FFI spellings `nros_cpp_service_client_send_request` /
  `_take_response` and C's `rclc_polling_client_send_request_raw` /
  `_take_response_raw` stay, for a caller with one request ever in flight; no
  in-tree example uses them now.
