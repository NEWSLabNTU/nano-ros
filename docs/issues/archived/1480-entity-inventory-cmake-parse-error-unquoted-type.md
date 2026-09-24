---
id: 1480
title: "The generated `nros_entity_inventory.cmake` has a service REQUEST type at a
  command position, so CMake refuses to parse it and `host-tests` now stops at
  `Build workspace fixtures` — one step past the 1468 wall it just cleared"
status: resolved
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

## Resolution (2026-09-24)

**The emitting line was not in the Rust renderer.** It is
`cmake/NanoRosEntityInventory.cmake`, which APPENDS the per-family inbox join to
the fragment the CLI wrote (phase-461 W3, issue 1352):

```cmake
string(APPEND _inbox_appendix
    "# NROS_DERIVED_SERVICE_INBOX_BYTES is ABSENT: ${_svc_why}\n")
```

`_nros_entity_inbox_bytes` returns a **multi-line** reason when a request type
carries no derived bound — one line per unbounded type, then a line telling the
reader how to bound it. A `#` comment in CMake runs to the end of ONE line, so
that interpolation commented the first line and left every later line at a
COMMAND position. The second line begins with the type name, which is exactly
the reported error.

That is why the search under "what this is NOT" came up empty: `render_received`
does quote both its joined values, and the CLI's own unit tests over individual
`set(...)` strings could never see this, because the offending bytes are written
by CMake after the CLI has finished.

It also explains the delayed report. The append happens inside
`nros_entity_inventory_read`, which does not re-include the file it just grew;
the parse error surfaces on the NEXT include — `_nros_entity_budget_env` in
`cmake/NanoRosEntityFacts.cmake` — so the function that wrote the bad line
succeeded and the stack named a different file.

**Fix.** One helper, `_nros_entity_comment_block`, comments EVERY line of the
reason, and both call sites (service and action) go through it. The two
families were one edit apart and fixing only the reported one would have left
the action family armed with the same defect (CLAUDE.md: fix the CLASS).

**Guard.** `tests/cmake-entity-inventory-tests.sh` case **I** builds a fragment
whose service request type is absent from the registered bound inventory, then
re-includes the appended fragment the way `_nros_entity_budget_env` does. The
assertion is the re-include, not the text — an appendix that reads correctly and
does not parse is the state that shipped. Negative control: with the helper
reverted the case fails with the reported error verbatim,
`Parse error.  Expected a command name, got unquoted argument with text
"example_interfaces/srv/AddTwoInts_Request".`

This answers both items under "what would close it": the emitting line is
pinned, and the whole rendered artifact is now fed to a CMake parse.
