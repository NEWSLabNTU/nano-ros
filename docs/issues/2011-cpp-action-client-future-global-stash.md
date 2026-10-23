---
id: 2011
title: "C++ action-client futures are unkeyed: results come through a process-global one-slot stash, and the goal-acceptance future races the arena dispatcher for its reply"
status: open
type: bug
severity: medium
area: [api]
found: 2026-10-23
related: [2009, 1996]
---

## What

Issue 2009 keyed every service-client reply by its request's sequence id,
including the action client's three sub-clients (`ActionClientCore`), so the
Rust core now attributes each goal acceptance and result to the goal its
request named. The C++ `rclcpp_action::Client<A>` FUTURES sit above that and
are still unkeyed (`rclcpp_action/client.hpp`, `send_goal_future` /
`get_result_future_sized` build `Future(storage, TryRecvFn, 0, executor)`):

- **Results go through a global stash.** The arena dispatch
  (`action_client_raw_try_process`) consumes every get_result reply during
  `spin_once` and fires `cpp_result_trampoline`, which copies the result into
  `RESULT_STASH` — a `static mut` single slot in `packages/api/nros-cpp/src/action.rs`
  shared by EVERY C++ action client in the process.
  `nros_cpp_action_client_try_recv_result` reads whatever is in it. Two
  `get_result_future`s (two goals, or two clients) can read each other's
  result, and a second result overwrites an unread one. `FEEDBACK_STASH` has
  the same shape.
- **The goal-acceptance future races the dispatcher.** The same dispatch polls
  `try_recv_send_goal_reply_for` unconditionally and hands the reply to
  `cpp_goal_response_trampoline`, which only forwards to a user callback.
  `Future<GoalAccept>` reads the core through
  `nros_cpp_action_client_try_recv_goal_response`, so a reply the dispatcher
  took during `Future::wait`'s `spin_once` never reaches the future; it
  resolves only when the reply lands between the spin and the poll.

No in-tree example or test drives `send_goal_future` / `get_result_future`
end to end (`action_goal_uuid.cpp` and `rx_size_bound.cpp` only compile them),
which is how neither surfaced.

## Direction

Per-client, goal-keyed delivery: the trampolines stash INTO the client (keyed by
`GoalId`, sized by `MAX_CONCURRENT_GOALS`) or into the waiting future the way
`rclcpp::detail::PendingList` does for service futures, and
`try_recv_result` / `try_recv_goal_response` take a goal id. A runtime test on
the stub backend with two goals in flight, as `client_seq_correlation_runtime.cpp`
does for services.
