---
id: 2009
title: "A service client does not match replies to requests by sequence number: two requests in flight cross-deliver, and a cancelled request's late reply answers the next one"
status: resolved
type: bug
severity: medium
area: [api, rmw]
related: [1996, 0778, 2011, phase-488]
found: 2026-10-23
resolved: 2026-10-23
---

## What happened

The RMW vtable has carried the request id both ways since issue 0778:
`send_request`'s `*sequence_id` and `take_response`'s `*seq_out`. Nothing
above it used the pair.

- **C++ `rclcpp::Future` was "slot 0".** `PollClient::send_request_sized`
  built every future with a literal `0`, and the nros-cpp FFI dropped the id at
  three layers: `nros_cpp_service_client_send_request` had no out-parameter,
  its body discarded the id (`Ok(_seq)`), and
  `nros_cpp_service_client_take_response` discarded it again. Two futures in
  flight on one client called the same take on the same storage, and whichever
  polled first got whichever reply landed first. A reply that arrived after
  `remove_pending_request` was taken by the client's next future (issue 1996's
  "Still open").
- **The same gap on every other surface**, as the stale-reply half. Each
  single-call path took "the next reply" and trusted it: the Rust `Promise`
  (after `reset_in_flight`), the executor's callback clients, C's
  `rclc_client_take_response` / `rclc_client_call` (a timed-out call's late
  reply answered the next call; the code comment called it "a known stale reply
  quirk"), the blocking `call_polling` / `nros_cpp_service_client_call_raw`, and
  the `nros` runtime's `call_raw`.
- **The action client attributed by `goal_counter`.** Goal acceptances and
  results were reported against the most recently SENT goal, so two goals in
  flight swapped acceptances, and an abandoned `get_result`'s late reply was
  delivered as the next goal's result (`action-client-multigoal`'s serial
  `reset_get_result_in_flight` loop could hit it).
- **C and Rust raw clients had nowhere to put the id.**
  `rclc_polling_client_send_request_raw` / `_take_response_raw` and
  `RawServiceClient::{send,take}_response_raw` returned bytes only.

## Resolution

One rule, applied everywhere a reply is taken: **a reply is delivered only to
the request whose id it carries; a reply naming no pending request is
dropped.**

- `nros_rmw::correlation` (new) is the rule: `RequestCorrelator<K, N>`, a fixed
  inline table of `(id, key)` pairs, plus `take_correlated` / `take_matched`
  (drain at most `MAX_STALE_DRAIN_PER_POLL` = 8 stale replies per poll).
- **C++**: `rclcpp::Future` is keyed by its request id and linked into its
  `PollClient`'s pending list (`rclcpp::detail::PendingList`), an INTRUSIVE
  list of the futures themselves. A future that takes another future's reply
  hands it over; a reply for a removed, destroyed or pruned future is dropped.
  Moves relink (future and client); a client destroyed under a pending future
  orphans it (`valid()` false) instead of leaving it pointing at freed storage.
  New FFI: `nros_cpp_service_client_send_request_seq` /
  `_take_response_seq` and `NROS_CPP_SEQUENCE_ID_UNKNOWN`; the old two
  forward. New `PollClient::prune_pending_requests()` (the ledger row said to
  re-open it once a table existed).
- **Rust**: `EmbeddedServiceClient` holds `RequestCorrelator<(), 1>` and the
  `Promise` takes through it; `reset_in_flight` abandons the id.
  `RawServiceClient` returns the id from send and take.
- **Executor callback clients** (C `rcl_client_t`, C++ dispatch `Client`,
  Rust `ServiceClientCallback`, `nros` runtime `call_raw`): the arena entry
  holds `ClientSequences` = the ids of its ONE logical call
  (`MAX_CLIENT_RESEND_GENERATIONS` = 4, because `rclc_client_call` re-sends
  every 500 ms and any generation answers the call). A timed-out call forgets
  its ids.
- **Action client**: each sub-client holds `ActionRequests` =
  `RequestCorrelator<GoalId, MAX_CONCURRENT_GOALS>`; replies are attributed to
  the goal their request named (C, C++ and Rust callback paths, and the C++
  goal-acceptance future's goal id).
- **C polled client**: `rclc_polling_client_send_request_raw_seq` /
  `rclc_polling_client_take_response_raw_seq` report the number, upstream's
  `rcl_send_request` / `rcl_take_response` shape.

**Bound, and what a full table does.** No knob. The C++ pending list has no
capacity of its own: its entries are the futures, so it is as long as the
futures in flight, and the transport's own outstanding-request limit caps that
(`NROS_ZENOH_MAX_PENDING_GETS`, Cyclone's 8 per client). A send past it is
refused by the backend and `send_request` returns an empty future, linking
nothing. A Rust/action table that is full refuses the send BEFORE it reaches the
wire (`NodeError::RequestInFlight`); the executor's resend table evicts its
oldest generation.

**Backends that cannot supply ids.** `NROS_RMW_SEQUENCE_ID_UNKNOWN`
(`INT64_MIN`) is new in `rmw_entity.h` (append-only, a `#define`, no slot
moves); the cffi adapter pre-sets both out-parameters to it, as it pre-sets
`NROS_RMW_TAKE_LEN_UNKNOWN`. A client whose last request went out without an id
keeps ONE request in flight (a second send is refused); an unkeyed reply goes to
the sole pending request and is dropped if there are several; a removed unkeyed
request costs the next unkeyed reply, which is dropped rather than delivered to
a newer request. No in-tree backend is id-less.

Tests: `client_seq_correlation_runtime.cpp` (`just check cpp`: out-of-order
replies both ways, late reply after removal / destruction / prune dropped,
moved futures, transport limit, id-less backend, client destroyed under a
future, `call_polling` vs a cancelled future); `client_seq_correlation.c`
(`just check c`); `nros_rmw::correlation` unit tests; nros-node
`promise_drops_the_late_reply_of_an_abandoned_call`,
`callback_client_drops_a_reply_for_another_request`,
`action_client_attributes_replies_by_request_id`. The stub backend stages
replies by id (`nros_stub_rmw_stage_reply_for`), caps outstanding requests and
has an id-less mode.

## Still open

- **C++ action futures** stay unkeyed in the header: results reach
  `get_result_future` through a process-global single-slot stash, and the
  goal-acceptance future races the arena dispatcher for the reply. Filed as
  issue 2011.
- The L1 polling action clients (`rclc_polling_action_client_*`,
  `nros_cpp_action_client_*_raw`) now drop stale replies but still return bytes
  only, so a caller with several goals in flight is not told which goal a reply
  is about.
- `.future` / `.request_id` / `share()` on the C++ future
  (`cpp:Client::async_send_request` stays ADOPT for these).
