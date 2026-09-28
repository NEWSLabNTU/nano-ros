---
id: 1555
title: "The `nros-metadata.json` `entities` key has no production producer — the
  reader survives only to build the declared-QoS compile fixture"
status: open
type: tech-debt
area: [tooling, build]
related: [1265, 1556, 1142, 1407, phase-403, phase-412, phase-454, rfc-0098]
---

## What

`ENTITIES` exists in this tree as **three populations**, and issue 1265 names
only one of them. This issue is the second: the `"entities"` key of
`nros-metadata.json`.

The cmake PRODUCER is gone. `_nros_metadata_emit()`
(`cmake/NanoRosNodeRegister.cmake`) lost the `_entities_field` splice point in
phase-454 W9, and `scripts/check/check-knob-single-reader.py` holds a
`Retired(...)` entry that forbids any cmake file writing the key again. The
Cargo-manifest spelling is separately refused by
`orchestration::nros_config::refuse_retired_entities_key`. A standalone leaf
states its entities in `system.toml` and reaches the pools through
`nros ws entity-facts`, never through this file.

The READER is still there:
`packages/cli/nros-cli-core/src/cmd/entity_inventory.rs`'s
`ComponentMeta::entities: Option<Vec<String>>`, consumed by
`inventory_from_metadata`.

## Measured (2026-09-29)

In a fully-populated checkout — `zephyr-workspace/build-*`, the example leaf
build dirs, the workspace configures:

```
$ for f in $(find . -name nros-metadata.json -not -path './packages/cli/nros-cli-core/tests/*'); do
      grep -q '"entities"' "$f" && echo "HIT: $f"; done; \
  find . -name nros-metadata.json -not -path './packages/cli/nros-cli-core/tests/*' | wc -l
```

| | count |
| --- | --- |
| built `nros-metadata.json` files scanned | **255** |
| of those carrying an `"entities"` key | **0** |

(The only hits anywhere are seven copies of one committed TEST fixture,
`tests/fixtures/refused_resolve/nros-metadata.json`, in agent worktrees. That
fixture carries the key decoratively — `refused_resolve_leaves_no_model.rs`
asserts nothing about entity counts.)

So on every real road the field is `None`, the declaration is
`Declaration::Absent`, and the metadata inventory contributes **no entities at
all**.

## What keeps it alive

One consumer, and it is a test input:

`packages/api/nros-cpp/tests/compile/declared-qos-fixture/entities.json` is a
committed `nros-metadata.json`-shaped document holding
`sub:std_msgs/msg/Int32:/chatter@depth=1`. `EntityInventory::
to_declared_qos_header` renders `nros/nros_declared_qos_generated.h` from it,
and that header is what `just check c` and `just check cpp` compile their
declared-depth `_Static_assert` TUs against — four `-I` call sites in
`just/check/lanes.just`, a positive TU that must compile and a probe TU that
must NOT. The committed header is held to the emitter by
`the_committed_compile_fixture_is_what_this_emitter_renders`.

Delete the reader and that gate loses its input.

## Why it is not a one-line deletion

`nros ws entity-inventory --output-header` accepts exactly two inputs:
`--metadata` (this key) and `--model` (a resolved SystemModel carrying a
contract sidecar's wiring). The real road is `--model`; in the production
call site (`cmake/NanoRosNodeRegister.cmake`) BOTH are passed and only the
model says anything.

The obvious conversion — commit a small SystemModel beside the fixture and drop
the metadata one — **is banned**. `check-no-tracked-models` (phase-330 W7.e)
refuses a tracked `*_model.yaml` / `*/system_model.yaml` outright, and all 99
resolved models in this tree are `system_model.yaml`, so the gate's reach does
cover it. The model is a build artifact; a committed one re-opens the issue-0380
hand-edit class.

## Fix direction (not decided)

Make the declared-QoS compile fixture's input what a real image's is: resolve a
tiny contract sidecar into a model at FIXTURE BUILD time (an
`examples/fixtures.toml` row, per "no compilation inside tests"), render the
header from `--model`, and then retire `ComponentMeta::entities`, its ~12 unit
tests and `entities.json` together — adding the reader to the existing
`check-knob-single-reader.py` `Retired(...)` entry so it cannot come back.

Cheaper alternative, and probably wrong: teach `entity-inventory` a `--leaf`
input reading `system.toml` `[[component]] entities` and restate the fixture as
a leaf. That retires population 2 onto population 3 — the surface issues 1265
and 1556 exist to retire — so it moves the debt rather than paying it.

## Acceptance

`just check c` and `just check cpp` still fail their probe TU (the negative
control must stay reachable), `ComponentMeta::entities` is gone, and
`check-knob-single-reader.py` refuses its reintroduction on the Rust side as it
already does on the cmake side.

## Corollary already recorded in the code

Because the field is always `None` in production, the documented reason for
`EntityInventory::merged_per_kind_max` — *"the contract has no timer entity, so
a model-only inventory under-sizes MAX_CBS by one per timer"* — describes a
contribution that is **empty today**. The merge is still load-bearing for a
different reason (`self.components` is the REGISTERED component population issue
1407 needs for the node table), but the entity terms come from the model alone.
Noted at the field in `cmd/entity_inventory.rs` so the next reader does not
trust the older comment.
