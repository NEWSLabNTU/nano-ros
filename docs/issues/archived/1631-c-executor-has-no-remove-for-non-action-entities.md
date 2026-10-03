---
id: 1631
title: "The C executor can add a subscription, timer, service or client and has no
  way to remove one — `fini` leaves the arena entry, the slot and `handle_count`,
  and for a client or timer the entry still dispatches through the C struct"
status: resolved
type: bug
area: [api, core]
severity: medium
found: 2026-10-02
related: [issue-1609, issue-1496, issue-1668]
---

## What happens

Issue 1609 gave the C executor its first two removal verbs,
`nros_executor_remove_action_server` / `_client`, built on the arena release
path issue 1496 added to `nros-node`. The other four entity kinds the C
executor registers have none, and their `fini` cannot fix it for the reason
1609 gives: `fini` is handed only the entity, while the arena entry,
`handle_count` and the trigger-entity table live in the `nros_executor_t`.

`scripts/check-c-executor-remove-coverage.py` (issue 1609) enumerates them
from the sources and carries each as a debt row naming this issue:

| entity type | registered by | arena entry's dispatch reaches |
| --- | --- | --- |
| `nros_subscription_t` | `nros_executor_add_subscription{,_typed,_typed_sized,_raw,_raw_with_info,_in_group}` | the user's callback + context only |
| `nros_timer_t` | `rclc_executor_add_timer`, `nros_executor_add_timer_in_group` | the user's callback, **handed the C `nros_timer_t *`** captured at registration |
| `nros_service_t` | `nros_executor_add_service{,_raw}` | the user's callback + context only |
| `nros_client_t` | `nros_executor_add_client` | `client_response_trampoline`, which **reads the C `nros_client_t`** |

So every kind leaks its callback slot (`NROS_EXECUTOR_MAX_CBS`), its
`handle_count` and its arena bytes on a create/fini loop, and its RMW entity
stays on the graph; the timer and client arms additionally dispatch through
the C struct after `fini`, which is a use-after-free when that struct was a
stack or freed object (the shape 1496 and 1609 closed for actions).

The node-created guard condition (`nros_node_create_guard_condition`) has the
same slot leak and the milder dispatch shape (`check-cpp-destroy-shape`'s
`nros_cpp_guard_condition_destroy` row describes it); it is not added through
an `*_executor_add_*` verb, so the coverage gate does not see it.

## What a fix needs

* `nros-node`: a release per kind — `release_entry(index, kind, size)` is
  generic already; what each kind needs is the concrete entry size its
  registration used (sized subscriptions/services carry const buffer
  parameters, so the C side must release with the same ones it registered
  with, as `nros_executor_remove_action_server` does).
* `nros-c`: `nros_executor_remove_{subscription,timer,service,client}`, in
  rclc's own shape (`rclc_executor_remove_subscription` etc. exist upstream),
  each undoing the arena entry, `handle_count` and the trigger table, then
  deleting its row from `UNREMOVABLE` in the gate (which fails on a stale row).
* The test is `packages/api/nros-c/tests/run/action_remove_cycles.c`'s shape:
  N add/remove cycles past `max_handles` and the arena with
  `nros_executor_get_arena_used` pinned at the first cycle, the stub's live
  entity count back to baseline, and a fini-only negative control.

## Resolution

Fixed 2026-10-03 on `fix/1631-c-executor-remove-all-kinds`.

**The release length is now RECORDED, not restated.** 1496's
`release_entry(index, kind, size)` took the size from the caller, who had to
name the concrete entry type — which works for an action entry and for nothing
here: a buffered subscription carries a runtime-sized receive ring after the
entry (12 120 bytes of arena for one stub subscription, of which the entry
struct is a fraction), and a timer's entry type is `TimerEntry<F>` where `F` is
`nros-c`'s anonymous wrapper closure. So `Executor::emplace_entry`, the one
choke point every registration goes through, now writes the region the entry
allocator actually handed out into `CallbackMeta::arena_len`, and
`release_entry(index, kind)` gives back exactly that. The field rides in
`CallbackMeta`'s tail padding — `size_of::<Option<CallbackMeta>>()` is
`6 * size_of::<usize>()` before and after, pinned by a `const` assert that
every target build evaluates (checked on `thumbv7m-none-eabi`), so no stated
executor backing size moves. The action releases keep their signatures; their
const generics stopped being load-bearing.

* `nros-node`: `Executor::release_{subscription,timer,service,service_client}(HandleId)`.
* `nros-c`: `nros_executor_remove_{subscription,timer,service,client}` in rclc's
  shape, sharing ONE executor-side tail with the two action removers
  (`forget_removed_handle`: trigger table + `handle_count`); per-kind counts
  (`subscription_count`/`timer_count`/`service_count`) come back down too, and a
  removed client returns from REGISTERED to INITIALIZED. NOT_FOUND for an entity
  not registered on that executor (or already removed); REENTRANT inside a
  dispatch. `nros_executor_add_subscription_in_group` now counts
  `subscription_count` like its siblings.
* `nros_generated.h` regenerated (`just regen-c-headers`).
* Ledger: `c:executor_remove_*` now declines only the guard condition; four
  `rust:Executor::release_*` rows in their topic shards.
* Gate: `check-c-executor-remove-coverage` — `UNREMOVABLE` is empty:
  `OK — 6 registered entity type(s): 6 removable, 0 tracked as debt.`

**Measured** (`just check c`, new probe
`packages/api/nros-c/tests/run/entity_remove_cycles.c`, stub backend in accept
mode, `max_handles = 4`, arena 74 240 B):

| kind | cycles | arena used, pinned at cycle 0 | fini-only control |
| --- | --- | --- | --- |
| subscription | 200 | 12 120 | fails add #4 (`-6` FULL), 4 still live |
| timer | 2 321 | 64 | fails add #4, 0 live (timers reach no backend) |
| service | 200 | 2 640 | fails add #4, 4 still live |
| client | 200 | 1 624 | fails add #4, 4 still live |

Each loop runs until it has claimed the arena at least twice; live backend
entities are back to baseline after every remove; a second remove and a remove
on a different executor both answer NOT_FOUND. Before this change the four
removers did not exist, and the fini-only control above IS the pre-fix
behaviour, measured in the same binary. Rust side:
`every_entry_kind_created_and_released_in_a_loop_never_exhausts`,
`a_buffered_subscription_releases_its_trailing_region_too`,
`a_kind_release_of_another_kind_is_refused`, `callback_meta_len_rides_in_padding`.

The stub backend gained a `supported_qos_policies` slot that answers CORE in
accept mode only (services and clients state RELIABLE and were refused); refuse
mode answers NONE, exactly as the NULL slot did.

**Not covered:** a callback CAPTURE stowed by `stow_capture` (the C++
`[this, state]` path) is a separate region and is not recorded — a release of
such an entry keeps those bytes. No C remover reaches one (C passes no
capture). `nros_executor_add_subscription_raw_with_info` takes no subscription
object and discards its handle, so nothing can remove what it registers, and
the gate cannot see it — filed as [issue 1668](../1668-c-sub-raw-with-info-unremovable.md).
The guard condition stays as the issue described it.

Sweep: `git grep -n 'pub unsafe extern "C" fn \(nros\|rclc\)_executor_add_' packages/api/nros-c/src`
and `git grep -n 'CallbackMeta {' packages/core/nros-node/src` (27 literals, all
recorded through `emplace_entry`).
