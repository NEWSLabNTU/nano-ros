---
id: 1668
title: "`nros_executor_add_subscription_raw_with_info` registers an arena entry and
  hands the caller nothing to remove it by — no subscription object, no handle"
status: open
type: bug
area: [api, core]
severity: low
found: 2026-10-03
related: [issue-1631, issue-1609]
---

## What happens

Issue 1631 gave every C entity the executor registers a remover
(`nros_executor_remove_{subscription,timer,service,client}`, beside 1609's two
action removers), and `check-c-executor-remove-coverage` now reports 6/6
removable. One registration path is outside that count by construction:

```c
nros_ret_t nros_executor_add_subscription_raw_with_info(
    nros_executor_t *executor, const nros_node_t *node,
    const char *topic_name, const char *type_name, const char *type_hash,
    const nros_qos_t *qos, nros_subscription_info_callback_t callback,
    void *context);
```

It takes no `nros_subscription_t`, and its success arm discards the
`HandleId` the arena returned (`Ok(_handle_id) => { handle_count += 1; ... }`
in `packages/api/nros-c/src/executor.rs`), so:

* the caller has no object to pass to `nros_executor_remove_subscription` and
  no handle to name the entry by — the entry, its slot, its arena bytes
  (payload buffer + attachment buffer) and its subscriber stay until
  `rclc_executor_fini`;
* the trigger-entity table records nothing for the slot, so
  `rclc_executor_trigger_one` cannot name this subscription either;
* the coverage gate's regex keys on the second parameter being
  `*mut nros_<kind>_t`, and here it is `*const nros_node_t`, so the gate does
  not see the verb at all.

Not a use-after-free (the entry's context is the caller's `context`, not a C
struct this API owns) — a leak on a create/destroy loop, and a hole in the
gate's reach.

## What a fix needs

* A shape that yields something removable: either an out-parameter
  (`nros_subscription_t *out`, the same object the other adds take — which
  also gives `trigger_one` an entity pointer), or a sibling verb taking a
  `nros_subscription_t*` with the existing one kept as a forwarder. Additive
  per RFC-0054 either way; regenerate `nros_generated.h`.
* `check-c-executor-remove-coverage` should count EVERY `*_executor_add_*`,
  and refuse one whose registration hands the caller nothing to remove by,
  rather than only those whose second parameter is an entity type.
* A cycle in `packages/api/nros-c/tests/run/entity_remove_cycles.c`.
