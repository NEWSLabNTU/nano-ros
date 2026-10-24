---
id: 2015
title: "Stock rclcpp_action code needs ~164 edited lines: no goal handles, no create_server/create_client, no SendGoalOptions callbacks — and a moved action server dispatches through dead storage"
status: resolved
type: enhancement
severity: medium
area: [api]
found: 2026-10-24
resolved: 2026-10-24
related: [2011, 1995, 1996, 1637, 1496]
---

## What

The rclcpp porting fixture (`packages/api/nros-cpp/tests/compile/rclcpp-examples/`)
measured 199 edited lines over 13 stock ros2/examples files, 164 of them in the
two action examples (server 77, client 87). Measured per hunk:

| cause | client | server |
|---|---|---|
| `ClientGoalHandle` / `ServerGoalHandle` alias and every callback taking one (`SharedPtr`, `std::shared_ptr<const Feedback>`, `WrappedResult`, `ResultCode`, `get_goal`, `publish_feedback`, `succeed` / `canceled`, `is_canceling`) — rewritten as static goal-id callbacks with `void*` contexts and raw-byte decoding | ~56 | ~45 |
| `rclcpp_action::create_client` / `create_server` over the four node interfaces, with `std::bind`-ed callbacks | 7 | 17 |
| `SendGoalOptions` upstream fields + `async_send_goal` | 14 | — |
| `Client<A>::SharedPtr` / `Server<A>::SharedPtr` members and their null checks | 5 | 2 |
| `using namespace std::placeholders` dropped | 2 | 2 |
| `feedback->sequence.back()` (no `back()` on generated sequences) | 2 | — |
| `std::thread{...}.detach()` per goal + `rclcpp::Rate` | — | ~9 |

Building the drop-ins exposed a real defect: `rclcpp_action::Server<A>`'s move
constructor and move assignment relocated the runtime storage but left the
executor's arena entry pointing at the MOVED-FROM storage
(`nros_cpp_action_server_relocate` copies bytes only). The client got the
re-point in issue 1496; the server never did. A server moved after
registration dispatched its first goal through the old bytes and rejected every
later goal (`goal_cb` read as `None`). Any code that moved a registered server
was affected; `create_server` returning by value made it unavoidable.

## Resolution

HOSTED drop-ins (`NROS_CPP_STD`), built on what landed in #2011 / #1995 / #1996:

- `rclcpp_action::ClientGoalHandle<A>` / `ServerGoalHandle<A>`: `SharedPtr` is
  `std::shared_ptr`; a handle is a heap token over the goal id (server handle:
  over `(server, id)`), forwarding to the entity and reading status from the
  arena. `WrappedResult` / `ResultCode` (upstream's values).
- `rclcpp_action::create_server` / `create_client` at upstream's paths, in the
  node-interface and node shapes; `Node::get_node_{base,clock,graph,logging,waitables}_interface()`
  return a handle naming the node (`rclcpp::node_interfaces`), used only by
  these. They return `X<A>::SharedPtr` = `rclcpp::Owned<X<A>>` (by value).
- `create_server` binds upstream's three `std::function` callbacks onto the
  server's existing `_with_ctx` slots (`detail::ServerUpstream`), keeping each
  live goal's message for `get_goal()`.
- `SendGoalOptions` carries upstream's three fields as
  `detail::UpstreamCallback<Sig>` (one pointer; a heap `std::function` hosted);
  `async_send_goal` binds them per goal on the client's existing dispatch
  hooks, requests the result once accepted when a result callback is given,
  and returns the goal-keyed `rclcpp::Future<GoalAccept>`.
- No layout follows the flag: hosted state sits behind `detail::HostedBox` /
  `UpstreamCallback` members that are the same width on every build
  (`check-cpp-capability-layout`); new hosted-family id `action-interop`.
- Freestanding: each shape is a compile-time refusal,
  `NROS_RCLCPP_REFUSE_ACTION_GOAL_HANDLE`, naming the goal-id-keyed form.
- The move defect: `Executor::retarget_action_server_raw` +
  `nros_cpp_action_server_retarget`, called from the server's move constructor
  and move assignment after the relocate.

The thread per goal stays a documented edit: a goal handle driven from a
thread the application started races the executor (RFC-0002; a one-level
image has no executor lock), and `rclcpp::Rate::sleep` from the accepted
callback would re-enter the executor. The port runs `execute(goal_handle)` in
the accepted callback.

Edit counts: action client 87 → 2, action server 77 → 6; total 199 → 43.
The client's 2 are `feedback->sequence.back()`: a `back()` on the generated
sequences would move the codegen version surface (RFC-0090) for one line, so
it was not added.

Tests (`just check cpp`): `action_upstream_dropins_runtime.cpp` drives every
drop-in through the real runtime on the stub backend, which now also stages
requests for a named service server and samples for a named subscription and
records each server's responses (`nros_stub_rmw_stage_request` /
`_stage_sample` / `_last_response`); it was the test that found the move
defect. Five freestanding must-fail probes
(`action_*_freestanding_probe.cpp`), one per refused spelling. The ported
examples compile, link and boot.

## Still open

- `async_cancel_goal` / `async_get_result` taking a goal handle, and
  `ServerGoalHandle::execute` (ACCEPT_AND_DEFER) — not used by the stock
  examples; ledgered (`cpp:ServerGoalHandle::execute` is a `gap`).
- The goal-slot leak: a cancelled or destroyed action future frees its Rust
  `ActionRequests` slot only when the reply arrives (needs a forget-by-goal
  path). Not touched here.
