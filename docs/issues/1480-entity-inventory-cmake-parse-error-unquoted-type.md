---
id: 1480
title: "The generated `nros_entity_inventory.cmake` has a service REQUEST type at a
  command position, so CMake refuses to parse it and `host-tests` now stops at
  `Build workspace fixtures` — one step past the 1468 wall it just cleared"
status: open
type: bug
area: cli, cmake, build, ci
severity: high
found: 2026-09-24
related: [1468, phase-461]
---

## What happens

`host-tests` push run **35978967757** (head `29264f364`), job **107566045321**,
step **`Build workspace fixtures`**:

```
CMake Error at cmake/nros/entity_inventory.cmake:191:
  Parse error.  Expected a command name, got unquoted argument with text
  "example_interfaces/srv/AddTwoInts_Request".
Call Stack (most recent call first):
  cmake/NanoRosEntityFacts.cmake:439 (include)
  cmake/NanoRosEntityFacts.cmake:753 (_nros_entity_budget_env)
  cmake/NanoRosEntityFacts.cmake:228 (nros_entity_facts_env)
  cmake/NanoRosEntityFacts.cmake:220 (_nros_entity_facts_flush)
  CMakeLists.txt:DEFERRED
-- Configuring incomplete, errors occurred!
Error: configure failed: `cmake -S build/posix-zenoh-native -B … -DNROS_RMW=zenoh`
  exited exit status: 1
  nros-cli-core/src/cmd/build.rs:1436:9
error: recipe `build-workspace-fixtures` failed on line 283 with exit code 1
```

The workspace is `examples/workspaces/cpp`, entry `native_service_server_entry`,
and the run's own log shows the inventory being derived immediately before:

```
-- nros: entity inventory DERIVED from 6 components -- 1 entities, 1 executor callback slots
-- nano-ros: sizing descriptor native_service_server_entry -- status partial, basis contract, 1 endpoint(s)
```

So the file is written, and then cannot be read back.

## Why it matters

This is the wall **behind** issue 1468. That regression stopped this lane at
`Build rust core fixtures` for fifteen consecutive runs; its fix merged at
~09:20 and the very next run got further — to `Build workspace fixtures` — and
stopped here. The lane still has no tier-1 verdict, and the reason has changed,
which is exactly the case where "same as last time" would have been wrong.

`example_interfaces/srv/AddTwoInts_Request` is a `pkg/srv/Name_Request`
spelling, which phase-461 W3 introduced (`service_request_types` /
`action_request_types`). The type names that were being emitted before that
wave do not contain the `_Request` suffix, so the emitter's new content is the
obvious suspect even though the exact line is not yet pinned.

## What this is NOT

- **Not 1468.** That was `unresolved import crate::parameter_services` at a
  Rust compile, two steps earlier, and it is fixed.
- **Not a quoting bug in `render_received`.** Read it: both
  `set(NROS_ENTITY_{what}_TYPES "…")` and
  `set(NROS_ENTITY_{what}_TYPE_COUNTS "…")` quote their joined value
  (`entity_inventory.rs:3818` and `:3823`). Whatever puts the type at a command
  position is elsewhere, so a reader should not "fix" those two lines and
  assume it is done.
- **Not a malformed type name.** `example_interfaces/srv/AddTwoInts_Request` is
  the correct wire spelling; CMake simply cannot have it start a line.

## What would close it

1. **Pin the emitting line.** The error is at a COMMAND position, so the
   candidates are a prose comment whose continuation line lost its `#`, a
   `set(...)` terminated early by an unescaped character, or a renderer that
   joins without quoting. `entity_inventory.cmake:191` in a failing build dir
   names it exactly; a unit test that renders one service entry and asserts the
   output parses would too.
2. **Assert the artifact parses, not just that it is written.** The generator
   has unit tests that check individual `set(...)` strings
   (`entity_inventory.rs:5107`, `:5113`); none of them feeds the WHOLE rendered
   file to a CMake parse. That is the gap this defect went through, and closing
   it is cheaper than the next instance.

Acceptance is `host-tests` reaching `just ci tier1` — a tier-1 verdict, green
or red, rather than a configure failure.
