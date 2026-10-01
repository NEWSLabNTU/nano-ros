---
id: 1600
title: "A multi-entry configure sizes its ONE runtime staticlib from the LAST
  entry's model, so `native_entry` of examples/workspaces/cpp dies at boot with
  `ExecutorFull`"
status: open
type: bug
severity: high
area: [build, cmake]
related: [1419, 1407, 1199, 1233, phase-412, phase-463]
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

## Direction

Fold per entry the way the accumulator already does for
`nros_record_entity_facts` (max of the counts, union of the infra flags), or
derive the fragment once over EVERY entry model of the configure, not over
the last one. Whichever: the fragment's own comment ("derived over every
component this configure registered") must become true of the MODELS too, and
a gate should build a two-entry configure whose LAST entry is the smaller one
and assert the delivered `NROS_DECLARED_EXECUTOR_MAX_CBS` is the larger.
Owned by whoever holds the cmake carrier road (issue 1407's area).
