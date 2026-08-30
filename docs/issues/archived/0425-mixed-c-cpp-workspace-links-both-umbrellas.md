---
id: 425
title: "A mixed C+C++ workspace links BOTH umbrella staticlibs and dies on ~96 duplicate symbols"
status: resolved
type: bug
area: cmake
related: [phase-241, phase-337, issue-0160]
resolved_in: "phase-337 session"
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

## Fix (landed)

Simpler than the INTERFACE-target restructure this filing sketched, and it
follows a rule the tree already applied in one place: **prefer the umbrella that
BUNDLES the other, whenever it exists.**

`NanoRosNodeRegister.cmake` already did this for a TYPED C component, with the
reasoning spelled out — "the umbrella bundles nros-c's C ABI, so `nros_*` C
calls still resolve, and only ONE Rust staticlib is linked". The bug was that
the same reasoning was applied only to typed components; every other site kept a
`NOT _TYPED` carve-out routing legacy C code to `NanoRos`. In a mixed workspace
that carve-out is what put both archives on one link line.

FOUR sites had the carve-out, and all four had to move together — fixing three
left the count at exactly 96 duplicates, because the last one still pulled the C
archive in:

| Site | What it links |
|---|---|
| `NanoRosNodeRegister.cmake` | the component library |
| `NanoRosVerbs.cmake` | the component library, `nano_ros_component_register` path |
| `NanoRosEntry.cmake` | the executable |
| `NanoRosGenerateInterfaces.cmake` | the GENERATED message-binding library |

The last is the one that is easy to miss: generated bindings are consumed by
both C and C++ packages, so they drag the C umbrella into a C++ executable no
matter what the node and entry choose.

A pure-C workspace instantiates no `NanoRosCpp` target, so every site falls
through to `NanoRos` exactly as before — verified, not assumed.

No `-z muldefs`: `check-no-allow-multiple-def` bans it repo-wide, and it would
have hidden a real ODR problem (issue 0160's class).

**Verified:** `c_mixed_workspace` links clean (was 96 duplicate symbols);
`pure_c_workspace`, `cpp_robot_entry` and `shadowing` all still exit 0;
`just build-test-fixtures lane=native` completes (was: stopped here);
`just check fast` + `just check build` green.

## Reproduce

```sh
just setup-cli && just build-test-fixtures lane=native
# or, narrowed:
#   the failing target is `robot_entry` from
#   examples/templates/c-and-cpp-mixed-workspace/
```

Confirmed on `c3fcdd7bf` with a clean `build-workspace-fixtures*` wipe, so it is
not stale build state.
