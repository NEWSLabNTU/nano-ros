---
id: 123
title: Mixed-workspace C node sources race the nros_config_generated.h byproduct under high parallelism (0088 residual)
status: resolved
type: bug
area: cmake
related: [0088, 0090, phase-258]
resolved_in: "compile-check-fixtures.sh pre-builds nros_{c,cpp}_config_header before the -j build"
---

## Resolved (2026-07-02)

`scripts/build/compile-check-fixtures.sh` now builds the `nros_c_config_header`
/ `nros_cpp_config_header` mirror targets FIRST (best-effort, `|| true` — absent
in header-less fixtures), then the full `-j` build. The real per-build header is
present before any consumer TU compiles, so the race can't occur regardless of
system load. Validated: full `just build-test-fixtures` green under the default
32-job budget (previously the c_mixed_workspace / shadowing fixtures hit the stub
in ~4 of 5 runs). The deeper option below (extend the 0088 `OBJECT_DEPENDS`
file-edge to every node-pkg source, or make the header a configure-time artifact)
remains the cleaner long-term fix but is no longer load-bearing for the fixture
build.

---


## Symptom

During a full `just build-test-fixtures` run, the `c_mixed_workspace`
cmake-fixture (`examples/templates/c-and-cpp-mixed-workspace`) failed while
compiling the C node package's *own* sources:

```
[54%] Building C object   src/c_talker_pkg/.../src/Talker.c.o
[54%] Building CXX object src/c_talker_pkg/.../nros-entry/main.cpp.o
[59%] Building C object   src/c_talker_pkg/.../nros_app_register_backends.c.o
...
nros/nros_config_generated.h:29:2: error: #error "nros_config_generated.h must be
    supplied per-build by the build system; see this stub for guidance."
nros/nros_generated.h:940:20: error: 'SESSION_OPAQUE_U64S' undeclared here (not in a function)
nros/nros_generated.h:1031:20: error: 'EXECUTOR_OPAQUE_U64S' undeclared here (not in a function)
... (every *_OPAQUE_U64S + 'unknown type name ActionServerRawHandle')
gmake: *** [Makefile:91: all] Error 2
```

The TUs picked up the committed in-tree **stub**
`packages/core/nros-c/include/nros/nros_config_generated.h` (`#error` guard)
instead of the per-build byproduct
`${CMAKE_BINARY_DIR}/nros-rust/nros-c-generated/nros/nros_config_generated.h`
(which `#define`s the `*_OPAQUE_U64S` inline-carve sizes consumed by
`nros_generated.h`). The `*_OPAQUE_U64S` macros are the FFI inline-carve sizes
(committed to `main` in `85d5aedce`, i.e. NOT introduced by the branch that hit
this).

## This is a 0088 residual, not a new class of bug

Issue [0088](archived/0088-zephyr-c-fixture-nros-config-generated-stub.md)
(resolved 2026-06-20) fixed exactly this stub race for native / cpp / mixed by
making the in-tree mirror a first-class `OUTPUT` +
`nros_{c,cpp}_config_header` target and having `NanoRosNodeRegister.cmake` add a
hard file-level `OBJECT_DEPENDS` Ninja edge + deferred `add_dependencies` on
`${_NRC_SOURCES}` consumers (component lib + carrier executables).
[0090](archived/0090-threadx-c-fixture-config-header-stub.md) mirrored it for
ThreadX. The residual here is that a **C node package's own build-target
sources** in the `c-and-cpp-mixed-workspace` path (`Talker.c`, the
`nros-entry/main.cpp` carrier, `nros_app_register_backends.c`) are not all
threaded through the `_nros_node_register_config_header_deps` /
`set_source_files_properties(... OBJECT_DEPENDS ...)` wiring, so under high
build parallelism a TU can still compile before the byproduct header lands.

## Reproduction / non-reproduction

- **Reproduced once** inside a full `just build-test-fixtures` run (max
  concurrency; the cmake-fixtures leg does `cmake --build "$bld" -j`, no `-j`
  cap — `scripts/build/compile-check-fixtures.sh`).
- **Did NOT reproduce** on an isolated `cmake -S … -B build/cmake-fixtures/c_mixed_workspace`
  + `cmake --build … -j` of the same fixture (clean, 100% built, rc=0), nor on
  the immediately-following full rerun (0 stub hits). → timing-dependent race,
  not deterministic.
- Suspected trigger: clearing `~/.cache` (sccache) beforehand removed compile
  caching, changing per-TU timing enough to lose the race. Scheduling variance
  alone is sufficient.

## Direction

Make the edge airtight rather than mostly-airtight — options:

1. Extend the 0088 `OBJECT_DEPENDS` file-level edge to cover **all** C/C++
   node-package build-target sources in a workspace (the `add_executable` /
   `add_library` a node pkg declares), not just the `${_NRC_SOURCES}`
   component-lib + carrier set, so no TU that `#include`s
   `<nros/nros_config_generated.h>` can start before the byproduct exists.
2. Or promote `nros_config_generated.h` from a cargo **build byproduct** to a
   **configure-time** artifact (generated during `cmake` configure, before any
   compile target exists) so the include is always present and no build-order
   edge is needed. This also removes the committed `#error` stub footgun.

## Notes

- Not a phase-271 regression: the `*_OPAQUE_U64S` carve + stub header are on
  `main`; phase-271 did not touch `nros_generated.h` / `cpp.rs`
  (`nros-build-helpers`). Surfaced only because `build-test-fixtures` was run
  after a cache clear.
- Non-blocking for `just ci`: the cmake-fixtures leg passes on retry; the race
  is rare and self-clears.
