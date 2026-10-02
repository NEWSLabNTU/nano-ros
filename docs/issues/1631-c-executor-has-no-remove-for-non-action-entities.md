---
id: 1631
title: "The C executor can add a subscription, timer, service or client and has no
  way to remove one — `fini` leaves the arena entry, the slot and `handle_count`,
  and for a client or timer the entry still dispatches through the C struct"
status: open
type: bug
area: [api, core]
severity: medium
found: 2026-10-02
related: [issue-1609, issue-1496]
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
