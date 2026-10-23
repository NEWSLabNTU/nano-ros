---
id: 2011
title: "C++ action-client futures are unkeyed: results come through a process-global one-slot stash, and the goal-acceptance future races the arena dispatcher for its reply"
status: resolved
type: bug
severity: medium
area: [api]
found: 2026-10-23
resolved: 2026-10-23
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

## Resolution

Per-client, goal-keyed delivery, reusing issue 2009's pieces rather than adding
a second correlation spelling:

- **C++ futures** (`rclcpp/future.hpp`). `detail::PendingLink` gains a goal key
  (`by_goal`, `goal_id[16]`), `detail::PendingList` gains `resolve_goal`, and
  `Future` a third mode: a GOAL-KEYED future linked on a list, whose poll is one
  pass of the action client's own dispatch (`nros_cpp_action_client_poll`).
  Moves relink; a destroyed client orphans its pending futures (`valid()` false).
- **`rclcpp_action::Client<A>`** holds two lists (acceptances, results) and keeps
  its OWN three callbacks installed (`on_goal_response` / `on_feedback` /
  `on_result`, context = the client). The executor's dispatch and the future's
  poll both reach those callbacks, so a reply lands in the future waiting on its
  goal whichever path took it; then it is forwarded to the user's callbacks
  (`set_callbacks` now stores them). A reply for a goal no future waits on is
  dropped from the futures (the user callback still sees it). Feedback goes to a
  per-client latest-feedback slot behind `try_recv_feedback` /
  `feedback_stream()`. No layout or ABI change: `CppActionClient` is untouched.
- **Rust FFI** (`nros-cpp/src/action.rs`). `RESULT_STASH`, `FEEDBACK_STASH` and
  the blocking calls' `BLOCKING_*` statics are gone. The blocking `send_goal` /
  `get_result` wait on a `BlockingWaiter` on their own stack frame, matched by
  goal id, and forward every other reply to the callbacks installed before
  (they used to unplug them and end on ANY acceptance/result). `poll()` frames
  payloads exactly as the arena dispatch does (shared `splice_encap`).
- **Bound.** Unchanged from 2009: the Rust core's per-sub-client `ActionRequests`
  (`MAX_CONCURRENT_GOALS` = 4) refuses a send past it (`RequestInFlight`), and
  the future is then empty. The lists are intrusive (entries are the futures),
  no heap.
- **L1 polling clients** (2009's leftover). New, append-only:
  `rclc_polling_action_client_try_recv_{goal_response,result,cancel_response}_raw_for`
  and `nros_cpp_action_client_try_recv_*_raw_for` write the goal a reply is
  about; the old entry points forward to them. `PollingActionClient` gains
  `goal_id_out` overloads.
- **Stub backend.** Each accept-mode client gets its own identity and a staged
  reply is taken only by the client that sent its request, so an action
  client's sub-clients and two clients stay apart, as on a wire.

Tests: `action_future_keying_runtime.cpp` (`just check cpp`): out-of-order
acceptances and results in both orders, the executor-dispatch path, two
clients with the same goal id, late results after cancel/destroy, a full table,
blocking calls with a pending future, a moved client, an orphaned future, and a
timeout's lower bound. Mutation-checked: making `resolve_goal` match any link
fails six checks. `client_seq_correlation.c` (`just check c`) covers the C
`_raw_for` entry points.

## Still open

- A cancelled or destroyed action future leaves its request in the Rust
  `ActionRequests` table until the reply arrives (the reply is then dropped).
  A server that never answers a `get_result` therefore holds one of the
  `MAX_CONCURRENT_GOALS` slots for good; a forget-by-goal path from the future
  to the core would close it.
- The futures assume one thread drives the client, as the service futures do:
  spinning the executor on another thread while a future is linked is not
  supported.
- `PollingActionClient::try_recv_result_sized` still skips a 5-byte prefix where
  the reply's payload starts at byte 8 (issue #179's offset); untouched here.
