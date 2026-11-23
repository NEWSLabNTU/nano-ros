---
id: 2886
title: "The island-shaped Zephyr snapshot the merge queue checks is stale: a live configure now derives more queryables, liveliness tokens and publishers"
status: resolved
type: bug
area: [ci, zephyr, sizing]
severity: high
found: 2026-11-23
resolved: 2026-11-23
related: [phase-590, 2493, 2772, 2618]
---

## What fails

The nightly's `check / template-copy-out [zephyr]` job (run 44988007873, 2026-11-22, main
`98b5d8e0c7`) fails `just check island-shaped-zephyr`:

```text
  - nros/entity_inventory.cmake: differs
      snapshot set(NROS_DERIVED_MAX_QUERYABLES 3)      live set(NROS_DERIVED_MAX_QUERYABLES 15)
      snapshot set(NROS_DERIVED_MAX_LIVELINESS 9)      live set(NROS_DERIVED_MAX_LIVELINESS 25)
      snapshot set(NROS_DERIVED_MAX_PUBLISHERS 3)      live set(NROS_DERIVED_MAX_PUBLISHERS 6)
      snapshot set(NROS_DERIVED_MAX_SUBSCRIBERS 3)     live set(NROS_DERIVED_MAX_SUBSCRIBERS 4)
      snapshot set(NROS_DERIVED_EXECUTOR_MAX_CBS 5)    live set(NROS_DERIVED_EXECUTOR_MAX_CBS 6)
island-shaped-zephyr: FAIL -- the snapshot the merge queue checks is stale
```

It first failed on run 44869354157 (2026-11-21, `7660227bcf`); run 44678975217 (2026-11-20,
`b28c0fbaca`) passed. Run 45115997343 (2026-11-23, `a1cbbb6b04`) and a local run at `b61082063c`
show the counts moved again: publishers 7 and liveliness 26.

## Lead

The growth matches every image now serving the stock node interfaces by default (six parameter
services, each a queryable, plus `/rosout`; the `+phase-556-w13-node-interfaces-default`
fragment). Each live number has to be checked against that change before the snapshot is
regenerated; a regeneration that only makes the check pass would hide a real increase.

## Every delta, accounted for

The image has two nodes, `controller` (C) and `monitor` (Rust). Measured at `b61082063c` with
`just check island-shaped-zephyr`; the snapshot was cut at `3ed64791a3`.

| knob | snapshot | live | entities | from |
| --- | --- | --- | --- | --- |
| `MAX_QUERYABLES` | 3 | 15 | +12: the six parameter services (`get`, `set`, `set_atomically`, `list`, `describe`, `get_types`) on `/controller` and on `/monitor`. The 3 transient-local cache queryables are unchanged | `6229085857` (phase-556 W13, stock node interfaces by default) |
| `MAX_PUBLISHERS` | 3 | 7 | +2 `/rosout`, one per node (`6229085857`); +2 `/parameter_events`, one per parameter node (`5e52799fe2`, issue 2493; under W13 alone it was one per executor, the 6 of run 44988007873) | `6229085857`, `5e52799fe2` |
| `MAX_SUBSCRIBERS`, `RMW_SUBSCRIBER_SLOTS` | 3 | 4 | +1 `/clock`, once per executor (`sim_time`, part of the stock set) | `6229085857` (the W11 family, on by default) |
| `EXECUTOR_MAX_CBS` | 5 | 6 | +1, the `/clock` subscription's callback | `6229085857` |
| `MAX_LIVELINESS` (and `ZENOH_MAX_LIVELINESS`, `-DZPICO_MAX_LIVELINESS`) | 9 | 26 | node tokens 3 (unchanged: 2 nodes + the primary) + publishers 7 + subscribers 4 + queryables 12 (15 less the 3 token-free cache queryables) | sum of the rows above |
| `CAPABILITY_PARAM_SERVICES` | 0 | 1 | the parameter family is on | `6229085857` |
| `MAX_PARAMETERS` | -- | 7 | the five parameters the launch file loads into `controller` plus `use_sim_time` on each node | derivation from `b8eb06593b` (phase-446 W4), emitted now the family is on |
| `PARAMS_MAX_NAME_BYTES` | -- | 13 | `max_steer_rad` | same |
| `MAX_ARRAY_LEN`, `MAX_STRING_ARRAY_LEN`, `MAX_BYTE_ARRAY_LEN`, `MAX_STRING_VALUE_LEN` | -- | 0 | no array or string parameter | same |
| `MEMORY_HEAP_BYTES` | 196608 | 114688 | the Kconfig default: the `RUST && (PARAM_SERVICES \|\| PARAM_STORE)` 192 KiB branch is gone | `617e65bd94` (issue 2772) |

Every count is what the stock node interfaces cost, on a model the real island matches: its
bringups (`actuation_module/src/controller_bringup/system.toml` and the three test bringups)
state `features = ["param_services"]` and no `node_interfaces`, so they take the stock set too.
The fixture keeps it; opting it out would stop it modelling the island.

## Two outcomes that were not intended

**1. The fixture's raised publisher pool fell below its own inventory.** The workspace exists
to state its pools ABOVE the derived counts (issue 2618's shape). With 7 derived, the stated
`knobs.zenoh.max_publishers = 6` is a cut, and the live configure says so:

```text
CMake Warning at zephyr/cmake/nros_cargo_build.cmake:654 (message):
  nros: NROS_MAX_PUBLISHERS=6, stated by Kconfig (CONFIG_NROS_MAX_PUBLISHERS
  in a .conf fragment or -D), is BELOW the 7 this image's entity inventory
  derives (7 = 3 publishers + 0 action-server publishers + 4 runtime
  publishers + 0 contract reporters).  The stated value wins, so the image is
  sized short of what its own inventory says it needs.
```

`check-knob-delivery` passes it (the knob is delivered as stated), so only the warning shows
it. Regenerating over it would have frozen a short-sized image into the snapshot the merge
queue checks. Not a runtime bug: the zenoh shim backs `/parameter_events` with one zenoh
publisher per session, so the image creates 6 and boots (below). But the fixture no longer
tested what it claims. The subscriber raise (6 over 4) still holds.

**2. The heap.** `617e65bd94` gives a Rust image with parameter services 112 KiB, and its
Kconfig help says a multi-node image may not fit. This one serves the parameter services on two
nodes, so it was measured: the live native_sim image, run 40 s against a private `zenohd`,
prints `nros: heap peak 87424 of 115200 bytes (27776 free at the peak)`, above the 24,576-byte
floor, with no entity-creation failure. Intended, and it fits, with 3,200 B above the floor.

## Fix

- `examples/workspaces/island-shaped/src/island_bringup/system.toml`: `knobs.zenoh.max_publishers`
  6 -> 9 on both `[image.freertos]` and `[image.zephyr]`, above the derived 7 again; comments
  name the derived counts and where the stock interfaces' entities come from.
- `just check island-shaped-zephyr-regen`, then the snapshot committed as the recipe wrote it.

## Acceptance

`island-shaped-zephyr` passes in a scheduled nightly. A release blocker:
[phase-590](../../roadmap/phase-590-nros12-release-plan.md) W1, blocker B4.
