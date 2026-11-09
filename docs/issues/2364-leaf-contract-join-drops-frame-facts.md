---
id: 2364
title: "The cargo-leaf contract join drops the model's tiers, monitor rows and infra services, so a leaf's descriptor describes less than it declares"
status: open
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
