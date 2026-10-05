---
id: 1668
title: "`nros_executor_add_subscription_raw_with_info` registers an arena entry and
  hands the caller nothing to remove it by — no subscription object, no handle"
status: resolved
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

## Resolution

Fixed 2026-10-05 on `fix/1668-raw-with-info-sub-removable`.

**A sibling verb, the old one kept.** `nros_executor_add_subscription_with_info(
executor, subscription, info_callback, context, invocation)` registers the same
raw + attachment arena entry into a `nros_subscription_t` the caller keeps
(initialised by `rclc_subscription_init_default`) — the info-callback twin of
`nros_executor_add_subscription_raw`, in rclc's argument order. The
subscription records `(handle, executor)` and the trigger table names it, so
`nros_executor_remove_subscription` releases it and `rclc_executor_trigger_one`
can name it. The direct-arg `nros_executor_add_subscription_raw_with_info` is
unchanged (RFC-0054 is additive) and its doc now says it is not removable.
`nros_generated.h` regenerated; parity-ledger row
`c:executor_add_subscription_with_info` added.

**One spelling for the bookkeeping.** The three subscription adds that take a
`nros_subscription_t` (`add_subscription`, `add_subscription_typed_sized`, the
new verb) now share `register_subscription_entry`, which does the validation,
name resolution, node routing and EVERY record the remover reads back, so a
fourth verb cannot register an entry the remover cannot find. Return codes are
unchanged (the plain add's NULL-callback check still runs after the state
checks). `nros_executor_add_subscription_in_group` keeps its own body: it does
not apply `sched_context_id`, and folding it in would change that.

**The gate reads every add.** `check-c-executor-remove-coverage` now parses
every `*_executor_add_*` FFI (23 today) and classifies each: keyed by an entity
type with a remover; keyed by an out-handle with a by-value remover (the two
shutdown hooks); `NOT_AN_ENTRY` (7: param metadata/constraints and the TT
major frame — the gate checks the body never touches `handle_count`); or
`SUPERSEDED` by a removable sibling (1: this verb). Four new self-test negative
controls. Measured:

- pre-fix `executor.rs` with the new gate, row present → `FAIL: SUPERSEDED row
  ... names nros_executor_add_subscription_with_info, which no
  *_executor_add_* defines`;
- pre-fix `executor.rs`, row removed → `FAIL: nros_executor_add_subscription_raw_with_info
  takes no entity object and hands back no handle`;
- fixed tree → `OK — 23 add verb(s) read; 6 registered entity type(s): 6
  removable, 0 tracked as debt; 7 register no entry, 1 superseded by a
  removable sibling.`

**Cycles.** `entity_remove_cycles.c` (in `just check c`) gains a `sub+info`
kind and a direct-arg control:

```
  sub+info        200 cycles, arena used pinned at 2008 of 74240 (entry 2008 bytes), live entities back to 0
  sub+info     control: fini alone failed add #4 with -6; 4 entities still live
  raw+info     control: direct-arg form failed add #4; 4 entities still live
entity_remove_cycles: OK
```

Sweep: `grep -n 'pub unsafe extern "C" fn \(nros\|rclc\)_executor_add_' packages/api/nros-c/src/*.rs`.

Not measured: a removal against a real backend (the stub counts creates and
destroys, which is the property); the direct-arg form still leaks by design —
a caller wanting removal moves to the new verb.
