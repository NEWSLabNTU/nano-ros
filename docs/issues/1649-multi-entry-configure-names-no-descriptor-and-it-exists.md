---
id: 1649
title: "A multi-entry cmake configure names no sizing descriptor to cargo — phase-457 W0.c kept that refusal because 'the condition does not occur', and it now occurs in a fixture the tier lanes build"
status: open
type: tech-debt
area: [build, cli]
severity: low
found: 2026-10-03
related: [1407, 1595, 1600, rfc-0100, phase-457]
---

## What

`nros_sizing_descriptor_cargo_env()` (`cmake/NanoRosSizingDescriptor.cmake`)
names EXACTLY ONE descriptor to cargo or none: a configure that declared several
entries has several descriptors and one shared runtime staticlib, and handing
cargo one of N would size the shared archive from one image. phase-457 W0.c
re-affirmed that refusal and recorded why it cost nothing:

> five `CMakeLists.txt` under `examples/` call `nano_ros_entry()`, each exactly
> once ... so no in-tree configure is in this state and it costs nothing today.

That premise no longer holds. Generated entries (phase-470) put several
`nano_ros_entry()` calls into ONE configure. Measured 2026-10-03:
`nros build --workspace examples/workspaces/cpp native` — the
`workspace-cpp-native` fixture's build dir, `build/posix-zenoh-native/cmake` —
writes FIVE descriptors (`native_entry`, `native_action_client_entry`,
`native_action_server_entry`, `native_service_client_entry`,
`native_service_server_entry`) and prints

```
nano-ros: 5 sizing descriptors in this configure and one shared cargo archive,
so none is named to cargo
```

Issue 1600 hit the same shape from the entity-inventory side (last entry won)
and fixed it by folding every entry's model into the union the shared runtime
must hold.

## What it costs

Every descriptor-first consumer falls back to its `NROS_DECLARED_*` carrier (or
its builtin) on that road. That is why the carriers cannot retire: with issue
1407's Zephyr west road closed, this road is the ONE every `Kept` row in
`scripts/check/check-knob-single-reader.py` still names. The carriers deliver
here because they reduce across entries (a MAX, or 1600's union); a descriptor
is a per-endpoint table with no such reduction.

No image is mis-sized by it today: the carriers answer, and they are correct
for the union.

## What closing it looks like

A descriptor for the SHARED runtime of a multi-entry configure — the same union
issue 1600 composes for the entity fragment (`EntityInventory::shared_runtime_over`)
rendered through `write_for_model` — named to cargo instead of none. Then re-run
the per-fact retirement test for every row this issue holds.
