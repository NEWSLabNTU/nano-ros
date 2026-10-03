---
id: 1661
title: "The cmake road compiles Cyclone with the DEFAULT `[types]` sizes — the descriptor states `max_fields` / `max_kinds` / `max_nested_depth` and nothing hands them to `nros_rmw_cyclonedds`, so D11's heap floor is computed from 256 types × 256 kinds"
status: open
type: tech-debt
area: [build, cmake, rmw]
severity: low
found: 2026-10-03
related: [1653, 1393, rfc-0100, phase-454]
---

## What

RFC-0100 D5: Cyclone reads `[types]` and `[target].heap_budget_bytes`, and
nothing else. On the cargo roads `WrittenDescriptor::cyclonedds_env` turns the
stated facts into `[env]` rows and `nros-rmw-cyclonedds-sys/build.rs`
(`forward_derived_knobs`) makes each a `-D` on the C++ compile.

On the cmake road the C++ TUs compile in the `nros_rmw_cyclonedds` cmake target
(or the Zephyr module library), and before issue 1653 NONE of the five reached
them. Issue 1653 forwarded the heap budget
(`nros_sizing_descriptor_apply_cyclonedds_heap`); the four `[types]`-derived
ones are still not forwarded: `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES` (whose
producer is `model_ingest::resolve_cyclonedds_max_descriptor_types`, a separate
single writer), `MAX_FIELDS`, `MAX_KINDS`, `MAX_NESTED_DEPTH`.

## Measured (2026-10-03)

`examples/workspaces/cpp` `freertos_posix` (cmake, Cyclone), with a temporary
`[board.knobs.memory] heap_bytes = 65536` on the `freertos-posix` board: the
descriptor states `max_fields = 1`, `max_kinds = 1`, `max_nested_depth = 1` (one `std_msgs/msg/Int32` image), and the image boots
with

```
nros-rmw-cyclonedds: configured heap 65536 bytes is below the 112640 this image
is certain to need (256 registered type(s), largest schema 256 kinds).
```

— the `heap_budget.hpp` defaults, not the image's facts. The floor is an
OVER-statement (the safe direction for a floor), but it is the descriptor's
facts not reaching the consumer they exist for.

## Direction

The same one-function shape as the heap budget: after
`nros_sizing_descriptor_read()`, put each STATED `NROS_SIZING_TYPES_*` on
`nros_rmw_cyclonedds` as the matching `NROS_CYCLONEDDS_*` definition (refused ⇒
nothing), and the descriptor-types count from its own single writer. Measure
the Cyclone stack arrays and the D11 floor before/after on the same image.
