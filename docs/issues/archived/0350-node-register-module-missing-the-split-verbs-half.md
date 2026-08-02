---
id: 350
title: "`NanoRosNodeRegister.cmake` does not include the module defining `nano_ros_auto_add_library`, so every direct-include consumer fails at configure time"
status: resolved
type: bug
severity: medium
area: build, cmake
related: [issue-0041, rfc-0048, rfc-0057]
---

## Finding (2026-07-29, while fixing issue 0342)

`scripts/build/compile-check-fixtures.sh` exits 1 on `main`:

```
CMake Error at CMakeLists.txt:7 (nano_ros_auto_add_library):
  Unknown CMake command "nano_ros_auto_add_library".
```

The build-stage fixture lane has therefore been failing wholesale — the script
is a prerequisite of `build-test-fixtures`, so anything it builds after the
failing fixture never ran.

## Cause

The RFC-0057 split spelling put its two halves in two modules:

| verb | module |
| --- | --- |
| `nano_ros_auto_add_library` | `cmake/NanoRosVerbs.cmake` |
| `nros_components_register_node` | `cmake/NanoRosNodeRegister.cmake` |

`NanoRosNodeRegister.cmake` never included `NanoRosVerbs.cmake`. A consumer
reaching it through `find_package(nano_ros)` gets both modules and never
notices; a consumer that includes it DIRECTLY gets half the spelling.

The build-stage fixtures include it directly, deliberately — issue 0041's
no-compilation-in-tests rule means they configure against the repo's cmake
modules by relative path rather than an installed package.

The 305-W2 sweep (`bb0b08419`) migrated 80 example/fixture CMakeLists from the
fused `nano_ros_node_register` — which lives in `NanoRosNodeRegister.cmake`, so
one include sufficed — to the split pair. The include did not follow.

## Four sites, not one

`l9_register_cpp` was the reported failure; `l9_register_c`,
`multi_pkg_workspace_px4/talker_pkg` and
`multi_pkg_workspace_px4/brake_arbiter_pkg` have the identical shape (direct
include + split verb). Fixing only the reported site would have left three
loaded guns — the CLAUDE.md "fix the CLASS, not the reported site" rule.

## Fix

`NanoRosNodeRegister.cmake` includes `NanoRosVerbs.cmake`. That module is
`include_guard(GLOBAL)`, so the include is idempotent and costs nothing on the
`find_package` path. No fixture edits: the module now provides what it
documents, so all four sites are fixed by construction, and a fifth written
tomorrow cannot reproduce the defect.

The alternative — adding a second `include(...)` line to each fixture — would
have fixed the same four sites while leaving the trap in place.

## Verification

Both `l9_register_{c,cpp}` configure cleanly, and
`scripts/build/compile-check-fixtures.sh` now exits **0** end to end (it had been
exiting 1 at this fixture).

## Why nothing caught it

The failing script is a BUILD-stage prerequisite, not a test — nothing asserts
its exit status in the test suite, so a red build stage only surfaces when
someone runs a fixture lane and reads far enough up the log. That is the same
gap issue 0309 describes one layer over: the signal exists but nothing is
watching it.
