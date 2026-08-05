---
id: 425
title: "A mixed C+C++ workspace links BOTH umbrella staticlibs and dies on ~96 duplicate symbols"
status: open
type: bug
area: cmake
related: [phase-241, phase-337, issue-0160]
---

## Symptom

`just build-test-fixtures lane=native` fails linking the mixed-language template
entry, with ~96 duplicate definitions of the whole C ABI:

```text
[100%] Linking CXX executable robot_entry
/usr/bin/ld: …/nros-cpp/libnros_cpp.a(nros_c.…rcgu.o): in function `nros_log_default_logger':
  packages/api/nros-c/src/log.rs:123: multiple definition of `nros_log_default_logger';
  …/nros-c/libnros_c.a(nros_c.…rcgu.o): first defined here
… nros_lifecycle_init, nros_lifecycle_fini, nros_lifecycle_get_state,
  nros_lifecycle_register_on_{configure,activate,deactivate,cleanup,shutdown,error} …
collect2: error: ld returned 1 exit status
```

This is a HARD blocker for the tier-1 fixture lane: the build stops, so every
test that consumes a native fixture reports missing/stale afterwards.

## Root cause — the invariant is right, the mixed shape violates it by construction

`cmake/NanoRosEntry.cmake:161` states the rule outright:

> The C and C++ umbrellas are now DISTINCT staticlibs (`libnros_c.a` vs
> `libnros_cpp.a`, one `std` each), so a C binary must link NanoRos (nros_c)
> and a C++ binary NanoRosCpp (nros_cpp) — **NEVER both**, or
> `std`/compiler-builtins collide.

Phase 241.D3-rev made `nros-cpp` bundle `nros-c` as an rlib dep so
`libnros_cpp.a` is the ONE Rust staticlib a C++ binary links. That is sound for
a pure-C++ binary. It is unsatisfiable for a MIXED workspace:

| Package | Source | Links |
|---|---|---|
| `c_talker_pkg` | `src/Talker.c` | `NanoRos` -> `libnros_c.a` |
| `cpp_listener_pkg` | `src/Listener.cpp` | `NanoRosCpp` -> `libnros_cpp.a` |
| `robot_entry` | `src/main.cpp` | both node libs, transitively BOTH umbrellas |

`nano_ros_entry`'s LANG inference picks `cpp` for the entry (correctly — its
source is `main.cpp`), but the C node package it links already dragged in the C
umbrella, and `libnros_cpp.a` contains the same `nros_c` objects. The entry
cannot satisfy "never both" by choosing its own LANG, because the second
umbrella arrives through a dependency.

`examples/templates/c-and-cpp-mixed-workspace/` exists precisely to prove this
shape works, so this is a supported configuration that the single-runtime change
made unlinkable.

## Fix direction

The final EXECUTABLE must pick exactly one umbrella; a node-package library must
not pick one at all. A C node library needs the C ABI **headers** (include dirs,
compile definitions) to compile, not the staticlib to link — the symbols come
from whichever umbrella the executable chooses.

So `nano_ros_auto_add_library` (and whatever else links `NanoRos` into a node
lib) should attach an INTERFACE-only target for the C ABI, leaving
`nano_ros_entry` as the single place an umbrella archive is selected. That also
restores the stated invariant instead of working around it with `-z muldefs`,
which `check-no-allow-multiple-def` bans repo-wide for exactly this reason
(issue 0160's class: a duplicate-symbol workaround hides a real ODR problem).

## Reproduce

```sh
just setup-cli && just build-test-fixtures lane=native
# or, narrowed:
#   the failing target is `robot_entry` from
#   examples/templates/c-and-cpp-mixed-workspace/
```

Confirmed on `c3fcdd7bf` with a clean `build-workspace-fixtures*` wipe, so it is
not stale build state.
