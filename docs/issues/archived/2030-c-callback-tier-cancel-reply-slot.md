---
id: 2030
title: "The C callback tier never takes a cancel reply, so the fifth rclc_action_cancel_goal is refused"
status: resolved
type: bug
area: [api]
severity: medium
found: 2026-10-24
resolved: 2026-10-24
related: [2027, 2009, 0796, 2034, phase-515]
---

**Home: [phase-515](../../roadmap/archived/phase-515-api-stock-shape-review.md)** — homed 2026-10-24; listed in that phase's table of open API issues.

## What

Found by inspection while fixing issue 2027 (the C++ goal-slot leak); not yet
measured.

Since issue 2009 each action sub-client keeps its requests in a bounded table
(`ActionRequests`, `MAX_CONCURRENT_GOALS` = 4), and a slot comes back only when
its reply is TAKEN. On the C callback tier nothing takes a cancel reply:
`rclc_action_cancel_goal` (`packages/api/nros-c/src/action/client.rs`) sends the
request, and neither the executor's arena dispatch
(`action_client_raw_try_process`: acceptance, feedback, result) nor
`rclc_action_client_poll` reads the cancel sub-client, and the callback tier
has no `try_recv_cancel_response` of its own (only the L1 polling client
does). So after four cancels the table is full even with a healthy server
answering every one, and the fifth is refused (`RequestInFlight`).

The C++ callback tier has the same shape but a reader:
`Client<A>::try_recv_cancel_response`, or since issue 2027 the
`async_cancel_*` futures, take the reply. A C++ caller that cancels and never
reads the reply hits the same wall.

## Fix direction

Either a cancel-response callback on the C client, dispatched by the arena
(which then needs a fourth callback slot in `ActionClientRawArenaEntry`, sized
in `nros-node/build.rs`), or drain-and-drop of cancel replies in the C path
when no reader exists. A C runtime test on the stub backend: five
`rclc_action_cancel_goal` with staged replies, all sent.

## Resolution

Measured first: the new C test sent the fifth cancel and got `RCL_RET_ERROR`
(`cancel 5 of 6 refused`).

- **A fourth callback slot in the arena entry.** `ActionClientRawArenaEntry`
  gained `cancel_response_callback: Option<RawCancelResponseCallback>`
  (`RawActionClientSpec` and `register_action_client_core` carry it). When it
  is `Some`, `action_client_raw_try_process` takes the cancel reply
  (`try_recv_cancel_reply_for`, which retires the request from `pending_cancel`
  and frees its slot) and calls the callback with the goal the request named,
  the reply's `CancelReturnCode` and its `goals_canceling` count. When it is
  `None`, the reply is left for the caller's own reader. The C++ tier passes
  `None` for that reason.
- **C registers it unconditionally** (`cancel_response_trampoline`, like the
  three other trampolines), so the executor's spin takes every cancel reply.
  `rclc_action_client_poll` takes them too.
- **New C API:** `rclc_action_client_set_cancel_response_callback(client, cb,
  context)` with `rclc_cancel_response_callback_t(goal_uuid, return_code,
  goals_canceling, context)`. It is optional, because the reply is taken either
  way. `rcl_action_client_t` gained the field; `nros_generated.h` was
  regenerated.

Test: `packages/api/nros-c/tests/run/action_cancel_replies.c`, in `just check
c`. It sends six cancels, each answered by a staged reply, first through
`rclc_executor_spin_some` and then through `rclc_action_client_poll`. Every
send must succeed, and every reply must reach the callback with its goal, code
and count. Mutation: registering the entry without the trampoline (the
pre-2030 shape) fails it at `cancel 5 of 6 refused`.

## Still open

- **The C++ callback tier is unchanged.** Its cancel replies are taken by
  `try_recv_cancel_response`, by the `async_cancel_*` futures, or by
  `poll()`, never by the executor's spin, so a C++ caller that cancels and
  never reads still fills the four slots. Taking them in the spin needs a
  per-request stash those readers could collect from. Filed as issue 2034.
