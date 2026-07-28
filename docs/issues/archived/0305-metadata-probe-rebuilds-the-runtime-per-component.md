---
id: 305
title: "The C/C++ metadata probe rebuilds the entire Rust runtime once per component — 2.6 GB and a cold build each"
status: resolved
type: bug
severity: high
area: cli, build
related: [phase-308, 0304]
resolved_in: "phase-313 W1 (4f6482685) — one probe project per workspace"
---

## Measurement (2026-07-28)

Probing `examples/workspaces/cpp` (six C++ components) after a partial run:

```
2.7G  cpp_add_client_pkg__add_client
2.7G  cpp_add_server_pkg__add_server
2.7G  cpp_fib_client_pkg__fib_client
732M  cpp_fib_server_pkg__fib_server   (killed mid-build)
----
8.8G  total, and only 3.5 of 6 components reached
```

Inside one probe:

```
2.6G  build/cargo          <- the whole Rust runtime, from scratch
 54M  build/nano_ros
 33M  build/component_pkg  <- the component itself
 22M  build/corrosion
```

**2.6 GB of the 2.7 GB is a private cargo target dir.** The component — the
only thing that differs between probes — is 33 MB.

### Where the 2.6 GB actually goes

Breaking the cargo dir down changes the cost model:

```
1.3G  debug/build/          <- build-script OUTPUT dirs
 615M   └─ nros-cpp .../sizes-probe-target-rustc-1.96.0-…
 (a second, comparable one under nros-c)
1.1G  debug/                <- the actual library build
172M  .../debug/deps
```

The single biggest item is a **nested cargo build inside the build script**:
`nros-c` and `nros-cpp` each compile the whole `nros` crate again to extract
`size_of::<T>()` for the generated sizes header. That is ~1.2 GB of the 2.6 GB,
and it runs once per probe PROJECT — six times for six components.

So the duplication is worse than "the runtime is rebuilt": the *sizes probe*,
which is itself a full nested build, is what is being repeated.

## Cause

`metadata_probe_cmake::run_probe` generates one CMake project per component,
each with its own `CMAKE_BINARY_DIR`. Corrosion derives its cargo target dir
from that, so every probe gets a private target dir and shares nothing: N
components means N cold builds of nros-cpp, nros-c, and their whole dependency
graph.

The runtime configuration is IDENTICAL across components in a workspace — same
backend, platform and ROS edition, all derived from the workspace, not the
component. There is nothing to justify rebuilding it per component.

## Why it went unnoticed

Unit tests assert the generated CMake text; they cannot see build cost. The
first real run was expected to take minutes and instead ran for hours, which is
how it surfaced. A wrong hypothesis about it costs an hour to disprove, which
is itself the reason two probe defects took so long to find (see phase-308).

## Fix

**One CMake project per workspace, N probe executables.** A single configure, a
single runtime build, and one small TU per component:

```cmake
find_package(nano_ros REQUIRED)
add_subdirectory(<pkg_a> component_a)
add_subdirectory(<pkg_b> component_b)
add_executable(probe_a probe_a.cpp)   # links component_a lib
add_executable(probe_b probe_b.cpp)   # links component_b lib
```

Each probe still writes its own sidecar, so the driver's per-component
`Ok/Err` reporting is unchanged; only the build is shared. Expected cost drops
from N cold runtime builds to one.

Build **per target** rather than the whole project — `cmake --build <dir>
--target probe_<i>` — so one component that fails to compile does not cost
every other component its sidecar. Configure is shared; failures stay
per-component, which keeps the driver's existing per-component `Ok`/`Err`
ledger meaningful.

The probe build dir already persists across `nros sync` runs, so once the
restructure lands the second and later syncs are incremental and near-free.

Interim mitigation if the restructure is deferred: point every probe at a
shared `CARGO_TARGET_DIR`. Whether Corrosion honours it needs checking — it may
override with its own `--target-dir`. This is a partial win at best: it would
share the library build but each project still re-runs configure, interface
codegen and the sizes probes.

## Adjacent, not this issue

The `sizes-probe-target-*` nested builds are ~615 MB EACH and are keyed by
rustc version + feature hash. They are recomputed per build tree for every
consumer of nano-ros, not just probes. A shared cache location for them would
help every C/C++ build in the tree. Worth its own issue if someone measures the
wall-clock share.

## Also worth fixing alongside

The probe build dirs are large and gitignored but never cleaned; a full
workspace probe leaves ~16 GB behind for six components.

## Resolution (2026-07-28)

Fixed by `4f6482685` ("feat(312-W1): one probe project per workspace, built per
target") — the restructure this issue proposed, landed by a parallel session.
`run_probes` now writes ONE `CMakeLists.txt` for the whole batch, configures
once, and builds each component with `--target probe_<i>` so a component that
fails to compile still costs only its own sidecar.

### Receipt

Same workspace, six C++ components, from a wiped `build/nros-metadata`:

```
before:  8.8 GB  and only 3.5 of 6 components reached
after:   2.9 GB  for all 6, "6 rebuilt, 0 already current"
```

So roughly a 5x reduction against the extrapolated before-cost (~15 GB for six),
and — more to the point — one runtime build instead of N.

### A correction to issue 0304

While fixing 0304 I reported that the probe "runs but records nothing (returns
-2), a separate recording-path defect". **That was wrong.** It was an artifact
of running a stale `nros` binary built before `4f6482685`. With a current
binary all six components record real entities — `talker.json` carries its node,
its `/chatter` publisher and the `std_msgs/msg/Int32` interface. There is no
recording-path defect; 0304's doc is corrected accordingly.

The lesson is the one this repo already documents for fixtures and repeats
here: a stale in-tree CLI produces failure signatures that look like code bugs.
`just setup-cli` before trusting any probe measurement.

### Still adjacent, still worth doing

The `sizes-probe-target-*` nested builds (~615 MB each, one per build tree for
nros-c and nros-cpp) remain the largest single item inside the now-shared tree.
Keyed by rustc version + feature hash, so they are cacheable across trees; that
is a separate optimisation, not this issue.
