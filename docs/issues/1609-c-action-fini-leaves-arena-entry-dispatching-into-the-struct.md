---
id: 1609
title: "nros_action_server_fini / nros_action_client_fini leave the arena entry
  registered and dispatching into the C struct — the C sibling of issue 1496,
  and the C API has no executor-remove call to fix it with"
status: open
type: bug
area: [api, core]
severity: medium
found: 2026-10-01
related: [issue-1496, issue-0810]
---

## What happens

Issue 1496 gave the executor arena a release path
(`Executor::release_action_server_raw{,_sized}`,
`release_action_client_raw{,_sized}`) and the C++ tier now uses it from
`~Server()` / `~Client()`. The C tier does not.

`nros_action_server_fini` (`packages/api/nros-c/src/action/server.rs`), in the
L2 `NROS_ACTION_SERVER_STATE_INITIALIZED` arm, says "action server lives in
executor arena (if registered) — reset metadata only" and returns. The arena
entry `nros_executor_add_action_server` registered stays live, with its
`context` = `&server._internal`. So:

* the action's three service servers and two publishers stay advertised;
* a goal arriving after `fini` dispatches `goal_callback_trampoline` through
  `server._internal`, which `fini` has just reset to `invalid_default()` — and
  if the caller's `nros_action_server_t` was a stack or freed object, through
  dead memory (the dangling-context arm 1496 closed for C++);
* `executor.handle_count` never comes back down, so a create/fini loop hits
  `max_handles` and then the arena.

`nros_action_client_fini` has the same shape (`context` = the client struct).

## Why it was not fixed with 1496

`fini` receives only the entity. `_internal.executor_ptr` is the executor's
`_opaque` pointer (`get_executor_from_ptr`), which is enough to reach the Rust
`Executor` and call the release, but NOT the owning `nros_executor_t`, whose
`handle_count` and `_handle_entities` registry also have to be undone. And the
C API has no `nros_executor_remove_*` for any entity — rclc's own answer is
`rclc_executor_remove_*`, which nano-ros does not provide. A fix that released
the Rust entry and left `handle_count` raised would make the loop fail one
knob later instead of fixing it.

## What a fix has to decide

* add `nros_executor_remove_action_server` / `_client` (rclc's shape), which
  receive the executor and can undo all three tables; or
* have `fini` release the Rust entry and give `nros_executor_t` a way to be
  reached from its `_opaque` (a back-pointer in `CExecutor`), so `fini` alone
  is enough.

Either way the test is 1496's: register / remove N times past
`max_handles` and the arena, with the arena's high-water mark pinned.
