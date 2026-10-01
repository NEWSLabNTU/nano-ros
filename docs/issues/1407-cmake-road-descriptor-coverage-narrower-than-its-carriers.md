---
id: 1407
title: "On the cmake road the sizing descriptor is written from a POORER
  inventory than the `NROS_DECLARED_*` carriers, is not written at all for a
  standalone leaf or a no-wiring model, and is withheld from cargo entirely in a
  multi-entry configure — three reasons a count carrier cannot retire"
status: open
type: tech-debt
area: [build, cli]
related: [1393, 1378, 1122, 1199, 1408, 1288, 0460, 1595]
found: 2026-09-21
---

## What is open

phase-454 W14 gave the sizing descriptor a second producer
(`nros_cli_core::sizing_descriptor::write_for_model`), so *a* descriptor now
reaches all three roads. W9 then asked the retirement question per fact — **can
the descriptor state this fact, on all three roads, today?** — and for the
COUNT-class `NROS_DECLARED_*` carriers the answer is no, for three mechanisms
that are independent of [issue 1393](archived/1393-cmake-road-has-no-bound-inventory.md)
and of each other.

1393 is about which FIELDS the model-only producer refuses. This is about **where
that producer runs at all, and what it is allowed to see when it does.**

## The three mechanisms, measured

### 1. Two producers, two different inventories — and only one can refuse

The `NROS_DECLARED_*` carriers come from `nros ws entity-inventory`, which
composes `inventory_from_metadata(nros-metadata.json)` and then folds the model
in with `EntityInventory::merged_per_kind_max`
(`cmd/entity_inventory.rs:219,257`). The descriptor comes from `nros ws
sizing-descriptor --from-model`, which builds
`EntityInventory::from_model(&model)` and **nothing else**
(`cmd/sizing_descriptor.rs:153`).

The component SETS therefore differ. `nano_ros_node_register()` puts a component
in the metadata; the contract sidecar puts one in the model. A component in the
first and not the second carries `Declaration::Absent`, and
`EntityInventory::derive` REFUSES for the whole image on exactly that
(`entity_inventory.rs:2312-2345`) — *"Deriving over only the components that did
would publish a slot count smaller than the image needs, and a short
NROS_EXECUTOR_MAX_CBS fails entity creation at boot."*

The descriptor's producer cannot reach that refusal, because it never sees the
metadata. Over the same image it derives happily over the contract's components
alone — a number smaller than the image needs, published as a stated fact. So
retiring the carrier does not merely lose a road; it replaces a mechanism that
refuses on incomplete data with one that cannot tell the data is incomplete.

### 2. No model ⇒ no descriptor, and the carriers are the road that failed

`write_for_model` is reached only when `EntityInventory::from_model` returns
`Some`, i.e. only when the model describes wiring — measured at W14, **5 of 114
resolvable models**. The other 109 get no file, correctly (an all-refused
descriptor would move the `[meta] basis` every consumer guards on).

But the carriers are not silent there. `NROS_DECLARED_NODES` and
`NROS_DECLARED_INFRA_QUERYABLES` are emitted UNCONDITIONALLY
(`cmd/entity_facts.rs:96-118`), and a **standalone leaf** — a copy-out cmake
project with no bringup and no SystemModel at all — gets its four queryable
facts from `facts_from_leaf` (`cmd/entity_facts.rs:295`), reading
`system.toml`'s `[[component]] entities`. That road has no model, so it can
never have a descriptor.

It is also the road that FAILED: issue 1378 measured
`examples/qemu-armv7a-nuttx/{c,cpp}/action-server` exhausting
`ZPICO_MAX_QUERYABLES` at boot, and `NROS_DECLARED_TL_PUBLISHERS` is the carrier
that fixed it. Retiring it would re-open 1378 on the only images that describe
themselves.

### 3. A multi-entry configure names NO descriptor to cargo, by design

`nros_sizing_descriptor_cargo_env()`
(`cmake/NanoRosSizingDescriptor.cmake:164-186`) refuses to hand cargo one of N
descriptors when a configure declared several entries against one shared
staticlib, and says so: *"the Rust half keeps its own defaults rather than sizing
every image from one of them"*. The entity facts have an answer for the same
collision — they take a MAX across the models — so in that configure the
carriers still deliver and the descriptor does not.

Measured 2026-09-21: five `CMakeLists.txt` under `examples/` call
`nano_ros_entry()`, each exactly once
(`grep -rc 'nano_ros_entry(' --include=CMakeLists.txt examples/ | awk -F: '$2>1'`
is empty), and each is in a separate project — so no in-tree configure is in
this state and it costs nothing today. It is the third independent reason the
replacement is not live on that road, not a present defect.

## Why this is not 1393

1393 is per-FIELD and its remedy is a bound inventory plus a board triple for a
model image. Every mechanism here survives that remedy untouched: a richer set
of FIELDS still comes from the poorer set of COMPONENTS, is still absent for a
leaf with no model, and is still withheld in a multi-entry configure.

## What CLOSING it looks like

* **(1)** Give `--from-model` the same composition the inventory verb has —
  take `--metadata` beside `--model` and run `merged_per_kind_max`, so the two
  producers share one inventory and one refusal. This is the one worth doing
  first and is largely plumbing.
* **(2)** A descriptor producer for a leaf that has `system.toml`
  `[[component]] entities` and no model. `facts_from_leaf` already reads exactly
  that declaration through the same `EntityDecl` grammar.
* **(3)** Per-entry descriptors on the cargo side of a multi-entry configure,
  which is really a question about the shared staticlib rather than about the
  descriptor.

## Do not

Do not retire a count carrier "because the descriptor states that field". The
field is not the question — **which inventory the producer read, and whether it
ran at all, is.** That is `check-knob-single-reader`'s own rule in the direction
1393 already records: a mechanism that still resolves is one people still use,
and removing it in favour of one that reaches less is the same defect inverted.

Each of the eight count-class carriers and the four queryable raw inputs is
registered as KEPT against this issue in
`scripts/check/check-knob-single-reader.py`, which is the ledger phase-454 W9
produced instead of a retirement.

## Progress, 2026-10-01 — two of three mechanisms closed; the open road is the Zephyr west ENTRY

| mechanism | state | where |
| --- | --- | --- |
| (1) two producers, two inventories | **CLOSED** — `--from-model` composes `--metadata` through `merged_per_kind_max`; what the poorer set cost was an UNDER-count (`node_count` 1 for 3 registered), not the refusal this issue predicted | phase-457 W0 |
| (2) no model ⇒ no descriptor | **CLOSED for the standalone cmake leaf** — `--from-leaf` over `system.toml` `[[component]] entities`, measured equal to the carriers on both 1378 leaves | phase-457 W0.b |
| (3) multi-entry configure | **decided: keep the refusal**, revisit when such a configure needs sizing | phase-457 W0.c |

**What is still open is a road this issue did not name, measured today:** a
**Zephyr west entry writes a descriptor and names none to cargo.**
`nano_ros_entry()` calls `nros_sizing_descriptor_from_model` on every road, so the
file exists in the Zephyr build dir; but `zephyr/cmake/nros_cargo_build.cmake`
— the only thing that hands knobs to the Zephyr cargo builds — has no
`NROS_SIZING_DESCRIPTOR` row (`git grep SIZING_DESCRIPTOR -- zephyr` is empty),
while it DOES forward the carriers: `NROS_DECLARED_TL_PUBLISHERS`,
`NROS_DECLARED_{SERVICE,ACTION}_INBOX_BYTES`, the three
`NROS_DECLARED_PARAM_NEEDS_*` and `NROS_DECLARED_PARAM_SERVICE_SHAPE`. On that
road the carrier is the only delivery, which is the safety island's road.

Closing it is the 0460/1288 problem, not a descriptor problem: the C lane
re-bakes its env into the command, and zephyr-lang-rust's
`rust_cargo_application` builds its own and inherits nothing, so the path has to
reach BOTH lanes, and it is a PATH (issue 0491 — watch the content, never
`rerun-if-env-changed` on the variable; `load_for_build_script` already does).
Turning it on changes what every descriptor-first consumer reads on every Zephyr
image with a contract, so it lands with a built-image measurement, not before.

### The ledger moved here when 1393 and 1408 closed

Re-run per fact on 2026-10-01 (the ledger's own rule when a blocker closes):

* the two INBOX carriers (from 1393) — read descriptor-first already; blocked
  only by the Zephyr west road above;
* six PARAMETER carriers (from 1408) — four forwarded on the Zephyr west road
  (the NEEDS and the service shape), two reached only by a multi-entry
  configure (on Zephyr those two travel as the knob itself). The three BOARD
  capacities left the issue-tracked ledger entirely (`ByDesign`, RFC-0100 D1).

23 carriers are KEPT against this issue now (`check-knob-single-reader` prints
the tally); many of the count and queryable rows also carry a structural reason the schema
cannot answer whatever the roads do (timers, the schedule, per-component
attribution, monitor rows), and those are recorded per row, not here.

### A correction to the ledger's `_LEAF_ROAD` reason

It said "a standalone Zephyr leaf reaches neither producer". True and not a
reason: `nano_rosConfig.cmake`'s Zephyr arm returns before
`nros_record_leaf_entity_facts`, so such a leaf gets no CARRIER either, and the
retirement test asks only about roads a carrier reaches. Rewritten to name the
Zephyr west ENTRY, which is the road that matters.
