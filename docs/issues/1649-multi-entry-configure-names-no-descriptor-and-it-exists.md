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

## Decided direction (2026-10-03, RFC-0100 Amendment 1, D12)

**The condition is structural, not incidental.** RFC-0065 D8 builds one cmake
configure per COORDINATE, so every image of a bringup that resolves to one
coordinate is an entry of one configure linking one runtime. The cargo road is
different: each image's generated entry is its own cargo root with its own
target directory. So N:1 is the cmake road's normal shape, and the answer is a
rule for it rather than an exception.

[RFC-0100 D12](../design/0100-rmw-agnostic-sizing-model.md#amendment-1-2026-10-03--the-unified-build-path-moved-under-this-model):
**the unit a descriptor sizes is the RUNTIME build.** Every runtime build is
named exactly one descriptor, and "exactly one or none" becomes "exactly one".
Of the three shapes this issue's title question allowed, D12 picks the first:

* **one runtime descriptor** — chosen. It is composed by the same composer over
  issue 1600's reduction (`EntityInventory::shared_runtime_over` over
  `NROS_ENTITY_INVENTORY_MODELS`, per component at `merged_per_kind_max`). That
  is the same model list and the same rule as the entity fragment and issue
  1564's declared-QoS table, so rows and `[image]` counts agree with what the
  carriers already deliver.
* per-entry descriptors plus a reduction in each consumer — rejected. That
  would put the reduction in every backend's `build.rs`, which is a second (and
  seventh) spelling of 1600's rule, and `from_build_env` names one path.
* one entry per configure — not this issue's call. It is RFC-0065 D8's trade
  (exact per-image sizing against N runtime builds), and neither side of it is
  measured. D12 holds under either answer.

Rules this issue's code must keep:

1. a per-endpoint fact the entries' models DISAGREE on is REFUSED, naming both
   models — never the max (the descriptor is the Rust declared-QoS check's only
   policy carrier, RFC-0100 D10);
2. the closure facts (`wire_bound_bytes`, `[types]`) need no reduction, because
   the configure's registered bound tables already ARE the runtime's closure;
3. `[meta]` names the runtime and lists the composed entries — a
   `schema_version` bump, shared with issue 1595's `[types]
   max_wire_bound_bytes`. Land both in one bump.

**Retirement after D12 lands** uses the W14 method: build
`examples/workspaces/cpp`'s native configure with the runtime descriptor named
and the carriers removed, diff each consumer's emitted knobs, and require no
difference. Because the descriptor states the union the carriers already carry,
that diff should be empty. **Not every row retires on D12 alone:** eleven rows
also lack a descriptor field, reader or file (issue 1655). Those stay until 1655
lands.

Files: `cmake/NanoRosSizingDescriptor.cmake` (`nros_sizing_descriptor_cargo_env`
and the composing call), `cmake/NanoRosEntityInventory.cmake` (the model list it
shares), `packages/cli/nros-cli-core/src/sizing_descriptor.rs` +
`cmd/sizing_descriptor.rs` (`--model` repeated, the D12 rules),
`packages/tooling/nros-sizing-descriptor` (schema), `tests/cmake-sizing-descriptor-tests.sh`
(a two-entry configure whose LAST entry is the smaller, asserting the runtime
descriptor is named and holds the union), and `scripts/check/check-knob-single-reader.py`
(the rows that retire).
