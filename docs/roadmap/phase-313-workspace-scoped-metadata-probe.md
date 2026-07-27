# Phase 313 — one probe project per workspace, not per component

**Status (2026-07-28): COMPLETE.** (Renumbered from 312 — a parallel session
had already taken that number for the launch-toolchain layering.) W1/W2/W3 landed; issue 0294 closes.

Resolved issue 0294, which had blocked phase-308 W1 from being testable:
probing a six-component workspace cost ~16 GB and six cold Rust builds, so a
single wrong hypothesis about the probe took an hour to disprove.

## The measurement

Probing `examples/workspaces/cpp` (six C++ components):

```
2.7G per probe project × 6  ≈ 16G

inside one:
  1.3G  debug/build/         build-script OUTPUT
   615M   └─ nros-cpp .../sizes-probe-target-rustc-1.96.0-…
          (a comparable one under nros-c)
  1.1G  debug/               the library build
   33M  component_pkg/       ← the only thing that differs
```

The component — the sole difference between probes — is 33 MB. The dominant
cost is a **nested cargo build inside a build script**: `nros-c` and `nros-cpp`
each recompile the whole `nros` crate to extract `size_of::<T>()` for the
generated sizes header. ~1.2 GB of nested work, repeated per probe project.

## Cause

`metadata_probe_cmake::run_probe` generates one CMake project per component,
each with its own `CMAKE_BINARY_DIR`. Corrosion derives its cargo target dir
from that, so nothing is shared. The runtime configuration is IDENTICAL across
components in a workspace — backend, platform and ROS edition all come from the
workspace, not the component — so there is nothing to justify the repetition.

Unit tests assert the generated CMake text and cannot see build cost, which is
why this surfaced only when a run expected to take minutes ran for hours.

## What does NOT change

**User package `CMakeLists.txt` files are untouched.** Each component package
is still built by its own CMakeLists via `add_subdirectory` — same verbs, same
interface libraries, same generated message headers. That property is what
makes "it compiled in the probe" mean "it compiles in an entry", and it is the
reason the recorded entity count can be trusted. The duplication being removed
is entirely in generated scaffolding under `build/`.

## Structure

Before — one generated project per component:

```
<ws>/build/nros-metadata/metadata-probe-cmake/
  talker_pkg__talker/{CMakeLists.txt, probe_main.cpp, build/ 2.7G}
  listener_pkg__listener/{CMakeLists.txt, probe_main.cpp, build/ 2.7G}
  … × 6
```

After — one generated project per workspace:

```
<ws>/build/nros-metadata/metadata-probe-cmake/
  CMakeLists.txt                       one project
  probe_talker_pkg__talker.cpp         one TU per component
  probe_listener_pkg__listener.cpp
  …
  build/                               ONE runtime, ONE pair of sizes probes
```

```cmake
find_package(nano_ros REQUIRED)
set(NROS_EXTRA_CPP_FEATURES "metadata-mode")

add_subdirectory(<ws>/src/talker_pkg   talker_pkg)     # its own CMakeLists
add_subdirectory(<ws>/src/listener_pkg listener_pkg)

add_executable(probe_talker probe_talker_pkg__talker.cpp)
target_link_libraries(probe_talker PRIVATE talker_lib NanoRos::NanoRosCpp)
…
```

The pattern is already proven in-tree: `examples/workspaces/cpp/src/zephyr_entry`
adds multiple sibling packages the same way.

## Waves

### W1 — batch the driver

`run_probe(one)` becomes `run_probes(&[…])`: render one CMakeLists naming every
component, one TU each, configure once.

**Build PER TARGET** (`cmake --build <dir> --target probe_<i>`), not the whole
project. Otherwise one component that fails to compile costs every other
component its sidecar — a robustness regression against today's independent
projects. Configure is shared; failures stay per-component, so the driver's
existing `Ok`/`Err` ledger keeps its meaning.

**Done when:** a workspace probe produces one build dir, and a component whose
sources do not compile leaves the other components' sidecars intact.

### W2 — prove the cost

Measure the same six-component workspace before and after. The claim to verify
is not "faster" but specifically: ONE `sizes-probe-target-*` pair for the whole
workspace instead of one pair per component.

**Done when:** the recorded numbers are in this doc, and a second `nros sync`
over an unchanged workspace is incremental (the build dir persists).

**Measured on `examples/workspaces/cpp` (six C++ components):**

| | before | after |
| --- | --- | --- |
| disk | ~16 GB (2.7 GB × 6, extrapolated) | **2.7 GB** |
| cold runtime builds | 6 | **1** |
| `sizes-probe-target-*` pairs | 6 | **1** |
| wall clock | hours (killed at ~90 min, incomplete) | **59 s** |

A second sync over an unchanged workspace reports `0 rebuilt, 6 already
current` — the content-addressed provenance stamp short-circuits it, no
rebuild at all.

The speedup is what made the phase finishable: three further reachability
defects were found and fixed after this landed, each in one round rather than
one per hour.

### W3 — the sidecars phase-308 W1 still owes

With iteration cheap, finish what 0294 was blocking: produce a real C/C++
sidecar and confirm `nros_orchestration_ir::sidecar_slots` counts it with no
language branch (it should need no code change at all — that is phase-308 W3's
actual claim).

**Done when:** `examples/workspaces/cpp` yields six sidecars and the phase-308
producer-gap ledger drops to zero.

**Done (2026-07-28).** Six sidecars, every field real:

| component | pubs | subs | timers | services |
| --- | --- | --- | --- | --- |
| talker | 1 | 0 | 1 | 0 |
| listener | 0 | 1 | 0 | 0 |
| add_client | 0 | 0 | 1 | 0 |
| add_server | 0 | 0 | 0 | 1 |
| fib_client | 0 | 1 | 1 | 0 |
| fib_server | 2 | 0 | 1 | 3 |

Both intercepts contributed, which is the design working: subscriptions,
services and publishers came through the recording RMW backend; the TIMERS on
four of six components came through the `nros-cpp` executor hooks. Timers never
reach the RMW, and they are precisely the entity the SystemModel is blind to —
the whole reason issue 0257 existed.

`language: "cpp"` is populated, the field that was a hardcoded `"rust"` literal
until phase-308 W2a.

**The consumption side needed ZERO code.**
`nros_orchestration_ir::sidecar_slots` counts a real C++ sidecar with no
language branch, because it keys on `(package, executable)` and counts entity
arrays. Locked in by `counts_a_real_cpp_sidecar_with_no_language_branch`, whose
fixture is the verbatim shape the probe emitted. Had C++ needed a special case
there, phase-308's "one mechanism, three front-ends" property would have been
false.

### Three reachability defects found on the way

All invisible to unit tests, all needing different fixes, all the same lesson —
"the code exists" is not "the code is in the image":

1. `nros-rmw-metadata` had **no dependents** — nothing linked it at all.
2. Made a dep, it was **DCE'd out of the rlib** — nothing referenced it.
3. Force-linked inside `nros-cpp`, it was still **omitted at the final link**: a
   static archive only contributes objects that resolve an undefined symbol, so
   the backend sat in `libnros_cpp.a` and never entered the binary. The probe
   now calls `nros_rmw_metadata_register()` explicitly, which both pulls the
   object in and registers without depending on `.init_array` ordering.

## Non-goals

- **Reusing the workspace's real build tree.** Tempting, but the probe needs
  `metadata-mode`, which changes the runtime feature set; mixing it into the
  user's build is exactly the layout divergence that broke every native C++
  build earlier (see the `defines_of` guard in `nros-build-helpers`).
- **A shared `CARGO_TARGET_DIR` across per-component projects.** Partial at
  best: it might share the library build, but each project still re-runs
  configure, interface codegen and both sizes probes — the expensive half.

## Adjacent, deliberately out of scope

The `sizes-probe-target-*` nested builds are ~615 MB each and are recomputed
per build tree for EVERY consumer of nano-ros, not just probes. A cache keyed on
rustc version + feature hash would help every C/C++ build in the tree. Needs a
wall-clock measurement first to know whether it justifies the complexity.

## Acceptance

- [ ] One generated CMake project per workspace; user package CMakeLists
      untouched.
- [ ] One `sizes-probe-target-*` pair per workspace, not per component.
- [ ] A component that fails to compile does not cost other components their
      sidecars.
- [ ] A second `nros sync` over an unchanged workspace is incremental.
- [ ] `examples/workspaces/cpp` produces six sidecars; issue 0294 closes.
