---
id: 1600
title: "A multi-entry configure sizes its ONE runtime staticlib from the LAST
  entry's model, so `native_entry` of examples/workspaces/cpp dies at boot with
  `ExecutorFull`"
status: resolved
resolved_in: 2026-10-02
type: bug
severity: high
area: [build, cmake]
related: [1419, 1407, 1199, 1233, 1564, 1607, phase-412, phase-463]
found: 2026-10-01
---

## Problem

A configure with several `nano_ros_add_executable(... LAUNCH ...)` entries
builds ONE runtime staticlib that every entry links, so its compile-time
tables must hold the LARGEST entry. `cmake/NanoRosEntityFacts.cmake` says so
("entries ACCUMULATE here ... and the union is applied once"), and
`_nros_entity_budget_env` says the fragment it reads "is ONE per configure,
derived over every component this configure registered".

The fragment is one per configure, but it is derived from ONE model:
`cmake/NanoRosEntry.cmake` calls
`nros_derive_entity_inventory_knobs(CLI ... MODEL "${_NRX_MODEL}")` once PER
ENTRY, each call rewrites the same `nros/entity_inventory.cmake`, and the
LAST entry's model wins. `_nros_entity_budget_env` then delivers that entry's
counts as `NROS_DECLARED_*` to the shared cargo build.

## Evidence

Measured 2026-10-01 on `examples/workspaces/cpp`, `nros build native`
(`build/posix-zenoh-native/cmake`, seven native entries, the last generated
being `native_service_server_entry`):

* `nros/entity_inventory.cmake` carries
  `NROS_ENTITY_INVENTORY_NOT_LAUNCHED "fib_client;fib_server;listener;add_client;talker"`
  and `NROS_DERIVED_EXECUTOR_MAX_CBS 1` -- the service-server model's answer
  (one service server, everything else not launched);
* `nano_ros/packages/api/nros-cpp/CMakeFiles/_cargo-build_nros_cpp.dir/build.make`
  passes `NROS_DECLARED_EXECUTOR_MAX_CBS=1` to cargo;
* `build/demo_bringup__native/resolved.toml` (the resolve phase, from the
  `system.launch.xml` model `native_entry` is generated from) says
  `max_cbs = 2` -- the talker's timer and the listener's subscription;
* booted against a router (`rmw_zenohd`, `NROS_ENTRY_SPIN_MS=1500`),
  `native_entry` prints `nros: NodeError::ExecutorFull` and exits 250;
* rebuilt with `NROS_EXECUTOR_MAX_CBS=8` stated in the environment (a stated
  value wins), the same binary constructs both nodes.

`cmake_cpp_workspace_entry_starts_prebuilt_runtime` does not see this: it
starts the fixture with NO router and asserts only that the process is still
running 300 ms later, which a process stuck in session open satisfies.

## Why it matters beyond the one workspace

Every multi-entry configure whose entries' models differ is sized for
whichever entry cmake processed last, which is an accident of SUBDIRS /
generation order. Whether the result is short (this case) or long depends on
that order, so it is the UNDER direction for some workspaces and silent
over-provision for others. It is the same symptom issue 1419 exists to catch
(`ExecutorFull` at boot) from a cause that no contract edit produced, and it
also cuts the phase-463 census short: the census producer IS the native image,
so a census run of `native_entry` stops at the same `ExecutorFull` (issue
1419's incomplete-census refusal reports it, correctly, as a census that
cannot confirm anything).

## Also measured (folded in from issue 1607)

Issue 1607 was filed the same day, independently, from issue 1564's work on
the same workspace, and is this bug. What it adds:

* **It is not new.** The same fragment and the same `ExecutorFull` were
  measured on a build of `examples/workspaces/cpp` dated 2026-09-29, before
  either issue's PR.
* **A test that spawns `native_entry` already fails on it:**
  `workspace_features::case_12_cpp_logging` exits in 0.4 s. Whether any CI
  lane currently reports that cell green was not measured.
* **A second comment states the false rule.**
  `cmake/NanoRosSizingDescriptor.cmake` says a multi-entry configure's
  "entity facts take a MAX across models for the same collision"; the
  inventory FRAGMENT does not. Both comments become true together or not at
  all.
* **The reduction has a home.** Issue 1564 gave `nros ws entity-inventory` a
  repeatable `--model` for the per-component compile-time tables (a union by
  endpoint), while the image-wide outputs still refuse a second `--model`
  because merging several images' counts was undefined. A per-kind MAX —
  `EntityInventory::merged_per_kind_max`, already used for metadata + one
  model — is the only reduction that cannot under-size; defining it for the
  image-wide outputs and calling the verb once per configure is one shape of
  the fix below.



Fold per entry the way the accumulator already does for
`nros_record_entity_facts` (max of the counts, union of the infra flags), or
derive the fragment once over EVERY entry model of the configure, not over
the last one. Whichever: the fragment's own comment ("derived over every
component this configure registered") must become true of the MODELS too, and
a gate should build a two-entry configure whose LAST entry is the smaller one
and assert the delivered `NROS_DECLARED_EXECUTOR_MAX_CBS` is the larger.
Owned by whoever holds the cmake carrier road (issue 1407's area).

## Resolution (2026-10-02)

The fragment is now derived over EVERY model the configure has seen, not the
last one.

* **cmake** — `nros_derive_entity_inventory_knobs` appends its `MODEL` to a
  GLOBAL list (`NROS_ENTITY_INVENTORY_MODELS`, deduplicated, missing paths
  dropped) and passes the whole list as repeated `--model`. Each entry still
  calls it once; the last call composes over all of them, and the readers that
  run after the entries (`_nros_entity_budget_env`, deferred) find that union.
  The intermediate rewrites settle: `nros_reconfigure_on_change` compares
  against the snapshot taken before the FIRST call of the pass (issue 1119).
* **CLI** — `nros ws entity-inventory` now defines the image-wide outputs
  (`--output-cmake`/`--output-json`/stdout env) over several `--model`s:
  `EntityInventory::shared_runtime_over` folds each wired model in through
  `merged_per_kind_max` (the rule one model already uses), so a component keeps
  the larger declaration per kind and a component ANY image launches is
  launched. Parameters reduce by `ParamDeclarations::union_over_images`: every
  image declaring → the union, every image silent → absent, any mix → refused,
  naming the silent ones. A model that describes NO wiring cannot be folded and
  does not say what it runs, so every row another model marked `NotLaunched`
  goes back to `Absent` and `derive` refuses, naming them. The per-component
  header path (`--component`, issue 1564) is unchanged.

The reduction is the UNION of the images, which SUMS where the true need is
the largest single image. That is the safe direction, and it is a real cost:
measured below, the arena advisory reports 752 of 66,816 bytes claimed at first
spin. A per-knob MAX over each image's own derivation would be tight, but the
fragment is not only numbers (declared-QoS tables, type lists, per-component
rows), so it would need a second, field-by-field reducer beside `derive` --
the shape this repo keeps paying for. Recorded here, not done.

### Measured

`examples/workspaces/cpp`, `nros sync` + `nros build native`, then
`scripts/build/workspace-fixtures-build.sh linux cpp --id workspace-cpp-native`:

| | before (main) | after |
| --- | --- | --- |
| `NROS_ENTITY_INVENTORY_NOT_LAUNCHED` | `fib_client;fib_server;listener;add_client;talker` | absent (every component is launched by some entry) |
| `NROS_DERIVED_EXECUTOR_MAX_CBS` | 1 | 9 |
| `NROS_DECLARED_EXECUTOR_MAX_CBS` on the nros-cpp cargo command | 1 | 9 |
| `native_entry` against `rmw_zenohd` | `NodeError::ExecutorFull`, exit 250 | boots; `Published: 0..5`, `Received: 0..4`, exit 0 |
| `workspace_features::case_12_cpp_logging` | fails (0.4 s) | passes (3.4 s) |

`examples/workspaces/c` and `mixed`: their fragments refuse, as they did
before — neither has a contract, so every model is unwired and there is
nothing to fold. (The `mixed` fixture build then failed in its BUILD step on a
cached `CMAKE_MAKE_PROGRAM` pointing at a deleted `third-party/make/gmake`,
the stale-cache class issue 1406 describes; unrelated to this change.)

### Gates

* `entity_inventory::shared_runtime_tests` (4): the shared runtime is never
  narrower than any image in either order; a component one image runs is not
  dropped by another; an unwired image makes the derivation refuse, naming the
  component (with a negative control); the parameter union/mix rules. Mutating
  the fold to keep only the last image turns two of them red.
* `tests/cmake-entity-inventory-tests.sh` case J: four calls (A, B, A, a
  missing path) — the last derivation names A and B exactly once each and no
  missing path; the first still names only its own. Reverting the cmake
  accumulation turns it red.
