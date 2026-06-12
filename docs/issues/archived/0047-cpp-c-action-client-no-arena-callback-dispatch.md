---
id: 47
title: C/C++ action client has no executor-arena callback dispatch (manual poll required); component layer lacks callback client bindings
status: resolved
type: enhancement
area: rmw
related: [rfc-0041, rfc-0043, phase-239, phase-240]
resolved_in: Phase 240.5 (commit on 2026-06-13)
---

## Resolution (2026-06-13)

The action client is now **callback-based** (RFC-0041 Principle). Added
`nros::bind_action_client<C, &C::on_goal_response, &C::on_feedback, &C::on_result>`
to `component.hpp` (+ the `nros_cpp_action_client_set_callbacks` callback typedefs
in the C `component.h`): it `create`s the client, registers the three member
callbacks by identity, and binds a component-owned **poll timer** that calls
`nros_cpp_action_client_poll` each spin tick — which drains the GET-query replies
and dispatches them into the member callbacks. Send goals with
`send_goal_async`; the acceptance + result arrive in the callbacks (no
`try_recv_*` poll loop, no re-entrant blocking `send_goal`/`get_result`).

Migrated `examples/qemu-arm-nuttx/{cpp,c}/action-client` to the callback shape;
re-added `Platform::Nuttx` to `test_rtos_action_e2e`. **Runtime-validated in
QEMU** (both langs): `Goal sent → Goal accepted by server → Result (status=4):
5 terms → Action completed successfully`; the test asserts `accepted=true,
completed=true`.

**Residual (minor, not blocking):** a `bind_service_client` callback helper is
not added — the service-client *poll* path already works (its reply channel is
pumped by the session each `spin_once`, unlike the action client's GET replies),
so its E2E passes today. Adding the callback helper for symmetry is a small
follow-up. A C/C++ FFI that arena-registers the action client (so `spin_once`
auto-pumps it without the per-client poll timer, matching Rust's
`register_action_client_raw`) is also a possible future cleanup.

---

## Original report

Per the [RFC-0041](../design/0041-unified-callback-receive-model.md) **Principle**
(callback by default; poll is a user-scheduling opt-in, never an RMW requirement),
every callback-capable entity should be **callback-bound** — the executor pumps
the transport once per `spin_once` and dispatches the user callback. This holds
for subscription, timer, service-server, action-server, **and clients**. The
**C/C++ action client** does not yet meet it, and the C/C++ **component** layer
exposes no callback-style client bindings at all.

## Evidence

- **Rust is correct.** `Executor::register_action_client_raw[_sized]`
  (`packages/core/nros-node/src/executor/action.rs:967`) arena-registers the
  action client with `InvocationMode::Always` (`action.rs:290,678`), so
  `spin_once` runs `action_client_raw_try_process` **every tick** and drives the
  goal-response / feedback / result callbacks automatically — no manual poll.
- **C/C++ FFI is incomplete.** `packages/core/nros-cpp/include/nros/nros_cpp_ffi.h`
  exposes `nros_cpp_action_client_set_callbacks(handle, goal_resp, feedback,
  result, ctx)` + `nros_cpp_action_client_poll(handle)` — but **`poll()` is NOT
  called by `spin_once`** (unlike the arena-registered sub/service paths). The
  auto-dispatch entry point `nros_cpp_action_client_register_async` is **only
  referenced in docstrings** (`nros_cpp_ffi.h:1501,1516`) — **no symbol is
  declared/implemented**. So a C/C++ action client's callbacks fire **only** if
  the user calls `client.poll()` itself each spin tick.
- **The poll trap.** A bare `nros_cpp_action_client_create` + `try_recv_*` loop
  with **no pump** (not arena-registered, `poll()` not called) receives nothing:
  nothing drains the reply channels. Mixing `poll()` (which dispatches into
  `set_callbacks` callbacks) with `try_recv_*` is contradictory — `poll()` drains
  the reply into the (possibly unset) callback, leaving `try_recv_*` empty.
- **Reproduced (phase-240.5 runtime E2E, 2026-06-13).** The NuttX cpp action
  *client* example (a poll component: `create_action_client_raw` +
  `send_goal_async` + `try_recv_goal_response`/`try_recv_result` each tick) sends
  one goal — the **server receives + completes it** (`Goal succeeded, rc=0`) — but
  the client never observes the goal-response/result reply. Service E2E passes
  (the service client's reply path is pumped); the action client's is not.

## Gaps

1. **No C/C++ arena auto-dispatch for the action client.** Implement the
   `register_async`-style FFI (the C/C++ analog of `register_action_client_raw`)
   so `spin_once` drives the action client's three reply channels — OR document
   that the component must call `client.poll()` each spin tick and wire that into
   the component binding.
2. **No callback-style client component bindings.** `component.hpp` /
   `component.h` have only the **poll** helpers `create_service_client_raw` /
   `create_action_client_raw`. Missing: `bind_service_client<C,&C::on_reply>`
   (reply callback) and `bind_action_client<C,&C::on_goal_response,&C::on_feedback,
   &C::on_result>` (set_callbacks + the required poll-each-tick or arena
   registration). These are the client analogs of `bind_subscription_raw` /
   `bind_service_raw` / `bind_action_server_raw`.
3. **Migrated NuttX client examples use poll.** `examples/qemu-arm-nuttx/{c,cpp}/
   service-client` and `action-client` (phase-240.5) drive `try_recv_*` from a
   timer. Per the Principle they should be **callback-based**; move them once the
   bindings above exist. (Service-client poll happens to work because its reply
   channel is pumped; action-client poll does not.)

## Direction

- Add the C/C++ action-client arena-dispatch FFI (or formalize the manual-poll
  contract) so the action client is callback-driven like sub/service.
- Add `bind_service_client` + `bind_action_client` to the component layer
  (callback by identity, no naming — RFC-0043 shape).
- Re-migrate the NuttX action/service client examples to callbacks; then re-add
  `Platform::Nuttx` to `test_rtos_action_e2e` (removed 2026-06-13 because the poll
  client could not receive replies).
- The **poll** API stays available for user-owned scheduling (RTIC / Embassy /
  task-per-entity) per RFC-0041 — this issue does not remove it.
