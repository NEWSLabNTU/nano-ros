---
id: 1607
title: "Duplicate of #1600 — a multi-entry configure sizes every entry from the LAST entry's model — `examples/workspaces/cpp`'s `native_entry` dies `ExecutorFull` at boot"
status: wontfix  # duplicate of issue 1600
type: bug
area: [build, orchestration]
severity: medium
found: 2026-10-01
related: [1600, 1407, 1564, 1393, phase-454]
---

> **Duplicate of [issue 1600](../1600-multi-entry-configure-sizes-runtime-from-last-entry.md)**,
> filed the same day from a different direction (issue 1564's work on the same
> workspace). Its distinct measurements and its fix direction were folded into
> 1600's "Also measured" section; track the bug there. The text below is kept
> as filed.

## What

A native workspace configure builds every entry of the workspace in ONE cmake
configure (`examples/workspaces/cpp/build/posix-zenoh-native/cmake` builds
seven), and every `nano_ros_entry()` calls

```cmake
nros_derive_entity_inventory_knobs(CLI "${_nros_bin}" MODEL "${_NRX_MODEL}")
```

(`cmake/NanoRosEntry.cmake`) with ITS OWN model, writing the ONE image-wide
fragment `nros/entity_inventory.cmake`. The entries are configured in the
order the generated top-level `CMakeLists.txt` lists them, so the fragment
every shared crate is sized from is whichever entry came LAST.

## Measured (2026-10-01)

On `examples/workspaces/cpp`, `scripts/build/workspace-fixtures-build.sh linux
cpp --id workspace-cpp-native`:

| | value |
| --- | --- |
| last entry in the generated top-level `CMakeLists.txt` | `native_service_server_entry` |
| `NROS_ENTITY_INVENTORY_NOT_LAUNCHED` in the fragment | `fib_client;fib_server;listener;add_client;talker` -- 5 of 6 components, i.e. the service-server model's view |
| `NROS_DERIVED_EXECUTOR_MAX_CBS` | **1** (one service server) |
| `native_entry` (talker + listener, two callbacks) against a live `rmw_zenohd` | exits 250 after `nros: NodeError::ExecutorFull` |
| `native_robot1_entry` + `native_robot2_entry` (one callback each) | run, 7 of 7 samples delivered |

Two callbacks against a derived `MAX_CBS` of one is consistent with the
`ExecutorFull`; the knob that actually reached `native_entry`'s executor was not
read back, so that link is inferred, not measured.

The same fragment and the same `ExecutorFull` were measured on the MAIN
checkout's build of that directory, dated 2026-09-29 19:12, so this predates
issue 1564's change (which touched neither the inventory fragment nor its
model selection). `workspace_features::case_12_cpp_logging` (which spawns
`native_entry`) fails in 0.4 s on this branch's build; whether any CI lane currently
reports that cell green was NOT measured.

## Why it matters beyond this workspace

`cmake/NanoRosSizingDescriptor.cmake` states, of a multi-entry configure, that
"the entity facts take a MAX across models for the same collision". The
measurement above says the entity-inventory FRAGMENT does not: it is
last-writer-wins, so an entry heavier than the last one is under-sized, and
the failure is a boot-time `ExecutorFull` (or a pool exhaustion) that names a
knob nobody set.

## Fix direction (not decided)

Compose the fragment once, after every entry, over every entry's model --
the per-kind MAX `merged_per_kind_max` already computes for metadata + one
model -- rather than once per entry with the last one winning. Issue 1564 gave
`nros ws entity-inventory` a repeatable `--model` for the per-component
compile-time tables only; the image-wide outputs refuse it on purpose, because
a merge of several images' counts was undefined. Defining it (a MAX per kind
is the only reduction that cannot under-size) is this issue.

## Acceptance

`native_entry` of `examples/workspaces/cpp` boots and delivers, built in the
same configure as its six siblings, and a gate fails when a multi-entry
configure's fragment is narrower than any one of its entries.
