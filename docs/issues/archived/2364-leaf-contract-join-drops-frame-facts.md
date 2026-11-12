---
id: 2364
title: "The cargo-leaf contract join drops the model's tiers, monitor rows and infra services, so a leaf's descriptor describes less than it declares"
status: resolved
type: bug
area: [cli, sizing]
severity: low
found: 2026-11-01
related: [rfc-0100, rfc-0074, phase-462]
---

**Needs measurement before any fix.** Folding these facts changes the knobs of
existing leaf images, so each one has to be measured, not assumed.

## What

On the cargo-leaf road (`sizing_descriptor::write_for_leaf`),
`contract_join::join` replaces the probe inventory's endpoint rows with the
contract's and keeps the probe's frame. The probe never sees a model, so every
frame fact that only the model knows is dropped on that road:

- `tiers`, the authored tier count;
- `monitor_rows`, the contract-monitor and age-monitor row counts;
- `infra`, the bringup's infrastructure services.

The model road merges the two inventories with
`EntityInventory::merged_per_kind_max`, which folds each of these (a max, a
per-table max, a union). The leaf road has no such fold, so the same
declaration yields a different descriptor depending on the road.

The ingress facts had the same defect and were fixed on their own in PR #2982
(`EntityInventory::adopt_declared_ingress`): the read-task budget, the declared
ingress list and the tightest tier's slack now survive the join. The three
facts above were left out on purpose.

## Why it was not fixed with the ingress facts

No in-tree leaf declares an ingress, so folding those changed no image. Tiers,
monitors and infra are different: a leaf that declares any of them would get a
different descriptor, and so different knobs, the moment the fold lands. Some
of these may already reach the image another way. `infra` has its own road
(`leaf_store_infra`, `leaf_infra`), so folding it again could double-count.

## What would close it

For each of `tiers`, `monitor_rows` and `infra`:

1. Find which in-tree leaves declare it (`system.toml` tiers, contract
   monitors, `[system] features`).
2. Build each such leaf's descriptor before and after folding the fact, and
   diff the knobs.
3. Fold the fact where the diff is the declaration finally being described.
   Where another road already carries it, leave it and say so in the join.

A test per folded fact, with a negative control, as
`a_declared_ingress_reaches_the_joined_frame` does for ingress.

## Fix (2026-11-12)

The facts were not only dropped by the join. Each leaf road applied its own
subset of them to the inventory before deriving anything:

| road | monitor rows | sched tasks | service families |
| --- | --- | --- | --- |
| cargo `[env]` sidecar (`leaf_env`) | yes | no | yes |
| west C-lane fragment (`west-leaf-sizing`) | yes | no | no |
| sizing descriptor (`write_for_leaf`, both roads) | no | no | no |

`leaf_entity_env::with_leaf_frame` now applies all three, by the model
road's rules (`EntityInventory::from_model`'s frame, and `with_fact_infra`
over `leaf_infra`), and every road calls it. The descriptor applies it to the
probe's inventory before the contract join, which keeps the probe's frame, so
the facts pass through the join. `with_leaf_monitor_rows` is gone.

Folding `infra` here does not double-count. The descriptor's queryable count
(`local_query_servers`) excludes the runtime's servers, and the families are
also stated separately (`infra_families`). The per-kind counts now match the
knobs the `[env]` road already wrote for the same leaf.

### Measured

None of the 29 in-tree cargo leaves declares a sched task, a contract
monitor or a service family, whether in `system.toml`, a contract sidecar or
a `param-services`/`lifecycle` feature on its `nros` dependency. `nros sync` ran
on every leaf twice: once with this CLI, and once with a build that differs
only in the descriptor's call. The descriptors are byte-identical on all 24
leaves that sync (15 of them with a probeable component). The other 5 are
test fixtures that fail `sync` under both CLIs for an unrelated reason: a
missing workspace member.

### Test

`the_leaf_frame_reaches_the_inventory_and_survives_the_join`:

- No model gives no change.
- A model with one `min_rate_hz` row, three tasks and the `lifecycle`
  feature states `monitor_rows = (1, 0)`, `sched_tasks = 3` and
  `lifecycle`.
- All three survive `contract_join::join`.
- The derived knobs move: one monitor row, plus two publishers (the
  `/diagnostics` reporter and `transition_event`).

Each fact was mutation-checked by deleting it, and the test then fails.
