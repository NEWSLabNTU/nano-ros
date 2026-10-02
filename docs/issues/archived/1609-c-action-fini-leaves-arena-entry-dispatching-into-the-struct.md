---
id: 1609
title: "nros_action_server_fini / nros_action_client_fini leave the arena entry
  registered and dispatching into the C struct — the C sibling of issue 1496,
  and the C API has no executor-remove call to fix it with"
status: resolved
type: bug
area: [api, core]
severity: medium
found: 2026-10-01
related: [issue-1496, issue-0810, issue-1631]
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

## Resolution (2026-10-02)

The first option, rclc's shape: the C API gains executor-side removal verbs.

* **`nros_executor_remove_action_server(executor, server)`** and
  **`nros_executor_remove_action_client(executor, client)`**
  (`packages/api/nros-c/src/executor.rs`) undo all three tables at once: the
  arena entry is dropped in place through 1496's
  `Executor::release_action_server_raw_sized` (with the SAME const parameters
  the add registered with — `MESSAGE_BUFFER_SIZE` x3, `NROS_MAX_CONCURRENT_GOALS`)
  / `release_action_client_raw` (default-sized, as the add), so the action's
  RMW entities are destroyed and its slot and bytes go back to the free list;
  `handle_count` comes back down; the trigger table's entity pointer is
  cleared. The entity's own record (`_internal`) is reset whatever the arena
  answers, so a second remove is `NROS_RET_NOT_FOUND` and can never release
  whatever registered into that slot since. Refuses `NROS_RET_REENTRANT`
  inside a dispatch and `NROS_RET_NOT_FOUND` for an entity registered on
  another executor (its `executor_ptr` is compared against this one's
  `_opaque`).
* **Why not "`fini` alone"** (the second option): every in-tree C example tears
  down `rclc_executor_fini` FIRST and the entity `fini` second, so a `fini`
  that reached back into the executor (by back-pointer or by `container_of`
  from `executor_ptr`) would touch a finalised — or, for a stack executor,
  freed — `nros_executor_t`. `fini` stays entity-local and its doc comment now
  says to call the remove first; `rclc_executor_fini` still drops every entry
  it holds, so a program that tears the whole executor down needs neither.
* **`nros_executor_get_arena_used` / `nros_executor_get_arena_capacity`** — the
  C API had no way to read the arena's high-water mark, so the measurement
  1496 made in Rust could not be made from C. Additive, beside
  `nros_executor_get_handle_count`.
* Headers regenerated (`cargo run -p nros-cbindgen-headers`, with
  `NROS_REPO_DIR` pointed at this checkout — issue 1280's inherited path
  otherwise names the main checkout); `nros_generated.h` gains the four
  declarations and the two `fini` doc notes. The book's C API reference
  (`book/src/reference/c-api.md`) names the teardown order.

### Measured

`packages/api/nros-c/tests/run/action_remove_cycles.c`, a compile+link+run TU
in `just check c` against the lane's `libnros_c.a` and the stub backend, which
gained an opt-in ACCEPT mode (`nros_stub_rmw_set_accept_entities`) so its
`create_*` succeed and are counted (`nros_stub_rmw_live_entities`) — the
refusal stays the default for every other probe. On the default sizing
(`max_handles = 4`, arena 74,240 B):

```
  server: 200 cycles, arena used pinned at 6712 of 74240 (entry 6712 bytes), live entities back to 0
  client: 200 cycles, arena used pinned at 5528 of 74240 (entry 5528 bytes)
  control: fini alone failed add #4 with -6; 20 entities still live
action_remove_cycles: OK
```

* 200 add / remove / fini cycles per kind; `nros_executor_get_arena_used`
  equals the FIRST cycle's on every later one (reuse, not headroom), and the
  precondition `200 x entry > capacity` and `200 > max_handles` is asserted;
* an action server is five backend entities, and after every remove the
  stub's live count is back to baseline — the action left the graph;
* **the negative control is the pre-fix teardown**: the same loop with `fini`
  alone fails at add #4 with `NROS_RET_FULL`, with 20 entities (4 x 5) still
  live; `rclc_executor_fini` then drops them all.

Mutations, each run against the TU: (1) skipping the arena release while
returning OK — `FAIL: server cycle 0: after remove handle_count=0 live=5`;
(2) dropping the client's `handle_count` decrement —
`FAIL: client cycle 0: after remove handle_count=1 live=0`.

### The class, and the gate

The class is "an entity the C executor can ADD has no way to be REMOVED", and
it is wider than actions: subscription, timer, service and client have no
remover either, and for the client (`client_response_trampoline` reads the
struct) and the timer (the user callback is handed the captured
`nros_timer_t *`) the entry still dispatches through the C struct after
`fini`. That is issue **1631**, filed with the per-kind table and the
`nros-node` work it needs (a sized release per kind).

`check-c-executor-remove-coverage` (`scripts/check-c-executor-remove-coverage.py`,
fast line via `just/check/abi.just`) is the C twin of `check-cpp-destroy-shape`:
every entity TYPE a `nros_executor_add_*` / `rclc_executor_add_*` FFI registers
must have a `nros_executor_remove_*` taking the same type, or an `UNREMOVABLE`
row naming an OPEN issue; a row whose remover has landed, or that no add
registers, fails as stale. Today: 6 registered types, 2 removable, 4 rows on
1631. Self-test (normal path): the compliant shape, a type with no remover and
no row (this issue's starting state), a stale row, a row naming a closed issue,
a row for an unregistered type, and the rustfmt-split signature.

### Not measured

* No C image on a TARGET ran a create/remove loop — host only, stub backend.
* No real RMW backend: the stub counts create/destroy calls; a real backend's
  undeclare (zenoh liveliness token, DDS endpoint) is the `Drop` 1496 already
  relies on, not re-measured here.
* Removal from inside a callback is refused, not supported.

Sweep: `git grep -n 'pub unsafe extern "C" fn \(nros\|rclc\)_executor_add_' packages/api/nros-c/src`
(and `python3 scripts/check-c-executor-remove-coverage.py`).
