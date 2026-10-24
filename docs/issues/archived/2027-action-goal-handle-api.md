---
id: 2027
title: "rclcpp_action's handle-taking client calls and ServerGoalHandle::execute were missing, and a dropped C++ action future held one of four goal slots forever"
status: resolved
type: bug
severity: medium
area: [api]
found: 2026-10-24
resolved: 2026-10-24
related: [2015, 2011, 2014, 2009, 2029, 2030]
---

## What

Issue 2015 left three things open on the hosted C++ action surface, and issue
2014 left a feedback decode untested:

1. **`Client<A>::async_get_result(goal_handle, cb)` / `async_cancel_goal(goal_handle, cb)`**
   (and `async_cancel_all_goals` / `async_cancel_goals_before`) did not exist;
   the ledger declined the stamp form.
2. **`ServerGoalHandle::execute()`** was a ledgered `gap`: nothing in the C++
   server moved an `ACCEPT_AND_DEFER` goal to EXECUTING. Worse, the core leaves
   every accepted goal ACCEPTED, so a `create_server` goal answered
   `ACCEPT_AND_EXECUTE` was never EXECUTING either, where upstream's server
   executes it before `handle_accepted`.
3. **The goal-slot leak.** Each action sub-client holds at most
   `MAX_CONCURRENT_GOALS` (4) requests in flight (`ActionRequests`, issue 2009),
   and a slot came back only with its reply. A future dropped first, whose
   reply never came, held its slot for good: after four abandoned goals (or
   get_result requests) every later send was refused.
4. `PollingActionClient::try_recv_feedback_sized` (fixed in issue 2014) and the
   C poll path's feedback arm had no runtime test.

## Resolution

**Client (hosted, `NROS_CPP_HAS_STD_PROMISE`).** `async_get_result` makes the
goal result-aware (requesting the result once) and returns the handle's own
future, as upstream does; `async_cancel_goal` / `async_cancel_all_goals` /
`async_cancel_goals_before` send `CancelGoal` (the latter two the zero id with
the zero stamp / with `stamp`, `ActionClientCore::send_cancel_request_stamped`)
and resolve to upstream's `CancelResponse` (`return_code`, `goals_canceling`,
decoded by `action_frame::cancel_goals_count` / `cancel_goal_info`). Each
returns `detail::DrivenSharedFuture<T>`, which IS-A `std::shared_future<T>`
and drives its client while polled or waited on (`is_ready`, `wait_for`,
`get`), because nothing fulfils it from another thread (RFC-0021).
`spin_until_future_complete` accepts it, and a bare `std::shared_future`, via
`rclcpp::detail::future_is_ready`. Correlation: results by goal on the
existing keyed path (issue 2011); cancel replies by the REQUEST's sequence id
(`try_recv_cancel_reply_seq`), because two cancels about one goal — or a
cancel-all beside a cancel-before, both keyed by the zero id — are otherwise
indistinguishable; the first test run caught exactly that. No new statics:
the waiters live in the client's heap `UpstreamState`.

`std::promise` is a capability of its own, `NROS_CPP_HAS_STD_PROMISE`: a
libstdc++ built without gthreads (arm-none-eabi) declares `std::promise` under
`-DNROS_CPP_STD` and never defines it, which `check cpp`'s ILP32 probe
measured. Without it the four calls are refused like a freestanding build.

**Server.** `nros_cpp_action_server_execute` (ACCEPTED -> EXECUTING via
`set_goal_status`, which publishes the status array; `NotFound` /
`NotAllowed` otherwise), `Server<A>::execute(goal_id)` on every build, and
`ServerGoalHandle::execute()`. `create_server`'s binding executes an
`ACCEPT_AND_EXECUTE` goal before `handle_accepted`. The ledger gap row is
gone.

**Leak.** The future is the request's owner. `PendingList` gained a forget
hook the action client installs; a goal-keyed future abandoned before its
reply (destroyed, `cancel()`ed, moved over) calls it, and the client forgets
one request about that goal in the Rust core
(`RequestCorrelator::remove_key` -> `ActionClientCore::forget_{send_goal,get_result,cancel}`
-> `nros_cpp_action_client_forget`) UNLESS another owner still waits on that
reply: a client-wide `set_callbacks` hook, or an upstream goal
(`async_send_goal` with callbacks — so the stock example's discarded future
does not cancel its own callbacks — or a result-aware handle). A late reply
for a forgotten request is dropped by the correlator. A feedback-only
upstream goal now holds its handle WEAKLY, so a caller that drops the handle
drops the entry (it used to stay forever); a result-aware one is held until
its result, as upstream's pending request holds it.

**Tests.**
- `packages/api/nros-cpp/tests/compile/action_goal_handle_api_runtime.cpp`
  (`just check cpp`): `async_get_result` (one request, future + callback,
  every future for a goal is the handle's), a discarded `async_send_goal`
  future still delivering, `async_cancel_goal` with `goals_canceling`,
  cancel-all and cancel-before replied out of order, a stray cancel reply kept
  for `try_recv_cancel_response`; `execute()` after `ACCEPT_AND_DEFER` with the
  published status array read back, a second `execute()` `NotAllowed`, an
  unknown goal `NotFound`, `ACCEPT_AND_EXECUTE` executing before
  `handle_accepted`; ten abandoned goals then an eleventh accepted past a late
  acceptance for the first, ten abandoned get_results then one that completes,
  six cancelled futures, and the client-wide-callback case keeping its slots.
  Negative control, measured: the forget hook removed -> 13 failures.
- `action_feedback_decode_runtime.cpp` (`just check cpp`): a staged feedback
  frame decodes to payload and goal id on `PollingActionClient`
  (`try_recv_feedback`, `_sized` + `GoalUUID`, a body that looks like a
  header) and on `Client<A>` through the executor's dispatch and
  `try_recv_feedback`. Negative control, measured: feedback decoded from byte
  0 (issue 2014's defect) -> 4 failures.
- `packages/api/nros-c/tests/run/action_feedback_decode.c` (`just check c`):
  `rclc_action_client_poll` hands the callback `[header][fields]` and the goal
  id; the L1 polling client returns the whole frame with the fields at
  `RCLC_ACTION_FEEDBACK_PAYLOAD_OFFSET`. Negative control, measured: the poll
  arm handing the bare fields -> 2 failures.
- Two freestanding must-fail probes (`action_async_{get_result,cancel_goal}_freestanding_probe.cpp`).
- Rust: `RequestCorrelator::remove_key` (nros-rmw), `action_frame`'s cancel
  reply readers, and `action_client_attributes_replies_by_request_id` now
  forgets a request and sees its late acceptance dropped.

Porting fixture: the action examples stay at 2 (client) and 6 (server) edited
lines — none of those edits was caused by these gaps (the client's is
`seq.back()`, the server's the thread per goal).

**Class sweep.** Service-client pending entries: the C++ `PendingList` of a
poll client holds no Rust slot (a dropped future unlinks, and the transport's
own limit bounds the request), so it has no such leak. The Rust `Promise`
leaves its entry until `reset_in_flight()` by design (phase 84.D3: an
abandoned call is acknowledged explicitly). The same class one channel over —
a cancel reply nobody takes — is issue 2030.

## Still open

- **Issue 2029** — a nano-ros SERVER ignores the zero-id cancel forms, so
  `async_cancel_all_goals` / `async_cancel_goals_before` cancel nothing against
  it (they work against an rclcpp server).
- **Issue 2030** — the C callback tier never takes a cancel reply, so its fifth
  `rclc_action_cancel_goal` is refused.
- A cancel reply is taken by its future or `Client::poll()`, not by the
  executor's spin (the arena dispatch has no cancel slot); a caller keeping
  only the `cancel_callback` must `poll()`.
- A result-aware goal whose result never comes keeps its get_result slot
  (upstream's pending request keeps it alive the same way); there is no
  terminal-status reclaim, because the client does not subscribe to the status
  topic.
- `get_goal_stamp` stays absent (the arena keeps no acceptance time).
