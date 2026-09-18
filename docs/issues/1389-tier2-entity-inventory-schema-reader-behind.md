---
id: 1389
title: "tier 2 dies configuring a persistent Zephyr workspace: the entity-inventory
  fragment states schema 6 and the READER answers 3, though producer and reader are
  both 6 on `main` — 1360's class with the direction reversed and a different artifact"
status: open
type: bug
area: [ci, build, codegen]
severity: medium
found: 2026-09-18
related: [1360, 1158, 1280, 1253, 0196]
---

## What happens

`run-matrix.yml` tier 2 (the 1-wise cover, 06:28 UTC) reaches the fixture build
and dies in **cmake configure** of a leaf, against the persistent workspace under
`~/.nros/workspaces/zephyr/3.7/`:

```
nros:
/home/runner/.nros/workspaces/zephyr/3.7/build-cortex-m-c-talker-zenoh/nros/entity_inventory.cmake
states entity-inventory schema version 6; this reader understands 3.
  Refusing rather than reading fields that may have moved.
  Rebuild the `nros` CLI so the producer and the reader come from one tree
Call Stack (most recent call first):
  .../cmake/NanoRosNodeRegister.cmake:173 (nros_derive_entity_inventory_knobs)
  .../cmake/NanoRosNodeRegister.cmake:150 (_nros_node_register_compose_inventory)
  CMakeLists.txt:DEFERRED
```

The guard is right to refuse. What is wrong is the number it compared against.

## Evidence

| what | value |
| --- | --- |
| run | 35315010126 (`run-matrix`, schedule, 2026-09-18T06:28:12Z) |
| head | `3a3ec205` |
| job | 105504675372 `tier 2 (1-wise matrix)`, step `just build tier2` |
| reporting job | 105509163214 — `tier 2 — NO VERDICT: stopped in the build` |
| leaf | `build-cortex-m-c-talker-zenoh` |

**The two numbers agree on `main`.** At `3a3ec205`:

* producer — `packages/cli/nros-cli-core/src/entity_inventory.rs:188`,
  `pub const ENTITY_INVENTORY_SCHEMA_VERSION: u32 = 6;`
* reader — `cmake/NanoRosEntityInventory.cmake:256`,
  `set(NROS_ENTITY_INVENTORY_SCHEMA_SUPPORTED 6 CACHE INTERNAL ...)`

So the artifact's `6` is what this tree's CLI emits, and the `3` the reader
answered with is **not a value that exists anywhere in the checkout that drove
the configure**. The fragment is current; the reader is stale.

That points at the storage rather than at either number: the reader is a
`CACHE INTERNAL` variable, so its value lives in the persistent build
directory's `CMakeCache.txt`, not in the module — and the build directory is
reused across runs and across the checkouts that configured it.

**This is exactly the hazard issue 1360 named and then removed from its own
reader.** 1360's resolution says of its first draft:

> The first draft of `nros_codegen_accepted_range` stashed it in a
> `CACHE INTERNAL` pair — this issue's own defect one layer up, since a
> persistent build dir would then answer with the range of whichever checkout
> configured it and a `MIN` bump would be invisible to the check that exists to
> notice it.

1360 fixed that for the codegen-version range (read on every call, never cached,
located from the module's own `CMAKE_CURRENT_LIST_DIR`). The entity-inventory
schema reader is the sibling that still has it.

## What this is NOT

* **Not 1360.** That is `NROS_EMITTED_CODEGEN_VERSION` on generated headers,
  resolved 2026-09-18 at `3fe1ef266`. Different artifact, different guard, and
  its fix does not read this variable. This failure appeared on the first tier-2
  run after that fix landed, which is consistent with 1360 having removed the
  error that used to mask it.
* **Not the opposite direction.** 1360 is a STALE artifact refused by a current
  tree. Here the artifact is CURRENT (schema 6, what today's CLI emits) and the
  reader is BEHIND (3). "Rebuild the `nros` CLI", which the message advises,
  would not change either number.
* **Not 1158.** The lane reached the build stage; the stage reporter says so
  (`stopped in the build`). This is not a provisioning failure.
* **Not a stale `nros` binary.** The fragment carries the version today's
  producer writes.
* **Not 1253.** That is the workspace manifest binding a foreign module; this is
  a cached cmake variable inside a build directory.

## What would close it

Measurement first — the exact provenance of the `3` is not established, and
there is more than one defensible fix site:

1. **Establish where the `3` comes from.** Print
   `NROS_ENTITY_INVENTORY_SCHEMA_SUPPORTED` and its source at configure time in
   that leaf, and read the persistent build dir's `CMakeCache.txt` for the
   variable. Either it is cached from an older configure of that directory, or
   an older module is on the include path ahead of the checkout's. The two have
   different fixes and the message cannot currently tell them apart.
2. **If it is the cache**: give the reader 1360's treatment — read the supported
   version on every call from the module's own `CMAKE_CURRENT_LIST_DIR`, never
   from a `CACHE INTERNAL` slot, and register the defining file in
   `CMAKE_CONFIGURE_DEPENDS` so a bump re-runs the configure. Per the 0196 rule,
   check whether the same shape covers `cmake/NanoRosMessageBounds.cmake`, which
   reads the same variable at `:787` and `:864`.
3. **Make the message name its own provenance.** It states both numbers but not
   where either came from; 1360's point 2 is the same complaint one artifact
   over. A reader that printed the file it read the supported version from would
   have answered question 1 without an investigation.
4. **Decide the lane's contract for the persistent workspace** — the same open
   question 1360 answered for generated trees (refresh when the producer moves).
   A cached reader variable is the second thing in that directory that outlives
   the tree that wrote it.

A gate is only worth designing after step 1: whether the right check is
"no schema reader may be `CACHE INTERNAL`" or something narrower depends on what
the provenance turns out to be.
