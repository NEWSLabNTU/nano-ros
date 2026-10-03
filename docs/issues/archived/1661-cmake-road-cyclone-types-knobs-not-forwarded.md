---
id: 1661
title: "The cmake road compiles Cyclone with the DEFAULT `[types]` sizes — the descriptor states `max_fields` / `max_kinds` / `max_nested_depth` and nothing hands them to `nros_rmw_cyclonedds`, so D11's heap floor is computed from 256 types × 256 kinds"
status: resolved
resolved_in: 2026-10-03
type: tech-debt
area: [build, cmake, rmw]
severity: low
found: 2026-10-03
related: [1653, 1663, 1393, rfc-0100, phase-454]
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

## Resolution, 2026-10-03

`nros_sizing_descriptor_apply_cyclonedds()` (`cmake/NanoRosSizingDescriptor.cmake`)
replaces issue 1653's heap-only function and puts every STATED Cyclone fact on
`nros_rmw_cyclonedds`: `[target] heap_budget_bytes` and `[types]` `max_fields`,
`max_kinds`, `max_nested_depth`, as the `NROS_CYCLONEDDS_*` definitions
`WrittenDescriptor::cyclonedds_env` produces on the cargo roads. Refused or
absent defines nothing (D6).

It also moved. 1653's first cut applied each ENTRY's descriptor as the entry
configured, onto ONE shared target — harmless for the board heap (every entry
states the same), wrong for `[types]`, where the last entry's `max_kinds` would
have won (issue 1600's shape on the C++ half). It now runs ONCE, from the
deferred entity-facts flush after every entry has registered, and reads the
descriptor the configure names to CARGO (`nros_sizing_descriptor_cargo_env`):
one entry's own, D12's runtime descriptor for several, a standalone leaf's when
there is no entry. The C++ and Rust halves of one runtime read one file.

**Measured** on `examples/workspaces/cpp` `freertos_posix` (cmake, Cyclone),
with the same temporary `[board.knobs.memory] heap_bytes` on the
`freertos-posix` board:

| | before | after |
| --- | --- | --- |
| `nros_rmw_cyclonedds` `flags.make` | `HEAP_BUDGET_BYTES=65536` | `+ MAX_FIELDS=1`, `MAX_KINDS=1`, `MAX_NESTED_DEPTH=1` |
| D11 floor at boot (`heap_bytes = 65536`) | `112640 ... largest schema 256 kinds` | `110600 ... largest schema 1 kinds` |
| `heap_bytes = 111000` | refused (below 112640) | boots, talker/listener deliver (`Published: N` / `Received: N`) |

On this road `MAX_KINDS` is the only one of the three with a C++ consumer
(`heap_budget.hpp`); `MAX_FIELDS` / `MAX_NESTED_DEPTH` size the Rust
descriptor builder's stack arrays, and no Rust Cyclone crate is compiled on the
cmake road (no `nros-rmw-cyclonedds` unit in the build dir). They are forwarded
anyway, for the same reason `forward_derived_knobs` forwards them: one table,
read the same way by both roads.

Test: `tests/cmake-sizing-descriptor-tests.sh` I1 (all four reach the target,
once), I2 (several entries: the runtime descriptor decides, an entry's cannot),
I3 (refused facts define nothing — negative control). Mutation-checked: a
heap-only forwarder fails I1; reading the last entry's descriptor fails I2.

**Not carried, and why — issue 1663.** `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES`
is not a descriptor fact, and its model-based writer is the wrong source on this
road: the cmake road registers every LINKED descriptor TU (36 in this image)
while the model counts one type, so deriving it from the model would
under-size the table. It stays the header's 256 here.
