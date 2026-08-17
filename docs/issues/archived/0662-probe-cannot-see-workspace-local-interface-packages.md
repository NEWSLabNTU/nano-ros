---
id: 662
title: "The C/C++ metadata probe runs before the workspace's own interface packages are built, so every component that `find_package`es one is unprobeable"
status: resolved
type: bug
severity: medium
area: cli, build
related: [issue-0641, phase-308, phase-313, phase-367]
---

## Symptom

Every C/C++ component in `examples/workspaces/features` — **16 of them** — has
no source-metadata sidecar and falls back to the SystemModel's entity lower
bound. `nros sync --verbose` says so, once per component, and nothing else
notices:

```
sync: source metadata — no producer for cpp_qos_talker_pkg::cpp_qos_talker
      (probe project configure failed: …)
```

## Cause — an ordering constraint, not a bug in the probe

The probe project configures with `CMAKE_PREFIX_PATH` set to the nano-ros
checkout and nothing else. The components do:

```cmake
find_package(custom_msgs REQUIRED)
```

`custom_msgs` is a WORKSPACE-LOCAL interface package
(`features/src/custom_msgs`). It is built by the workspace's own CMake build —
which runs **after** `nros sync`, because sync is what generates the inputs that
build consumes. At sync time there is no `custom_msgsConfig.cmake` anywhere and
no install prefix in the workspace, so `find_package` cannot succeed.

Verified rather than assumed: `find … -name 'custom_msgs*Config.cmake'` returns
nothing, and the workspace has no `install/` prefix.

So these components are unprobeable **by construction at sync time**, not
because anything is malformed.

## What was fixed here (phase-367 W4)

The failure used to be all-or-nothing: one component's `find_package` error
aborted the whole batch configure, so *every* component in the workspace lost
its sidecar. That contradicted this driver's own contract, which it states for
the BUILD step a few lines below:

> ONE unprobeable component degrades to the sidecar-less path by NAME rather
> than taking the whole workspace with it.

It held for builds (there is a per-target fallback) and not for configures,
where the code asserted the opposite: *"A configure failure IS fatal: it means
the project itself is malformed, not that one component is unprobeable."*

`run_probes` now drops the components CMake NAMED and retries with the rest,
using CMake's own attribution (`CMake Error at <dir>/CMakeLists.txt:N`, where
`<dir>` is a component's `package_dir`) rather than a guess. An error naming
nothing keeps the old fatal behaviour, because a project really can be
malformed and picking a victim would be worse.

**In this workspace it changes no outcome** — the retry loop drains 16 → 12 →
11 → 5 → 4 → 1 and every component fails for the same missing package. That is
a property of `features`, not of the mechanism, and it is why this is filed
rather than closed.

### Cost, stated

| | before | after |
| --- | --- | --- |
| cold (markers cleared) | ~1 configure | 5.5 s, 6+ configures |
| warm (negative cache, issue 0641) | — | 0.10 s |

The cold path is ~5x more expensive on a workspace where nothing can configure,
paid once per source change because issue 0641's negative marker absorbs the
repeat. That is a real trade and it is made on the contract, not on speed: a
workspace with one broken component and sixteen good ones now gets sixteen
sidecars instead of none.

## Resolved 2026-08-17 — it is not a dependency cycle, it is a missing search path

The framing above ("unprobeable by construction", "an ordering constraint") was
wrong, and the question that broke it was *"isn't this a circular dependency?"*
— because checking whether it IS circular is what turned up the answer.

It is not circular. `custom_msgs` is a **verbatim upstream ROS 2 msg package**:

```cmake
find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)
rosidl_generate_interfaces(${PROJECT_NAME} msg/Reading.msg)
```

Both dependencies come from the ROS installation. **Nothing it needs is produced
by `nros sync`**, so there is no cycle to break — sync does not wait on anything
that waits on sync.

And the workspace's own `CMakeLists.txt` says exactly how the real build
resolves it, without building or installing `custom_msgs` at all:

```cmake
# the compat layer auto-emits its Find-stub for packages under this
# search path (Phase 210.A.2). MUST precede `find_package(nano_ros)`.
set(NROS_INTERFACE_SEARCH_PATH "${CMAKE_CURRENT_SOURCE_DIR}/src")
```

The probe project simply never set it. `render_probe_cmakelists` now emits the
documented pairing — `set(NROS_INTERFACE_SEARCH_PATH <ws>/src)` **before**
`find_package(nano_ros)`, and `nros_workspace_interfaces()` **after** it, since
nano_ros is what defines that function.

**The order is load-bearing and cost a wrong attempt.** Emitting both together
after `find_package(nano_ros)` compiled, configured, and still failed with the
same `custom_msgs` error — the compat layer reads the variable while the package
is being found (`NanoRosCodegenCore.cmake`). The workspace's comment said "MUST
precede" and meant it.

The search root is derived rather than passed: a component lives at
`<ws>/src/<pkg>`, so its parent IS the root, and `probe_dir_for_workspace` keys
one project to one workspace.

### Measured

| | before | after |
| --- | --- | --- |
| `features` C/C++ components probed | 0 of 16 | **16 of 16** |
| reported unsupported | 16 | **0** |
| sidecars written | 0 | **16**, with `callbacks`, `nodes`, `parameters`, `provenance` |
| cold sync | 5.5 s (all failing) | 44.5 s (16 probes actually BUILD now) |
| warm sync | 0.10 s | 0.11 s |

The cold number went UP because the probe now does the work it was always meant
to: sixteen probe executables get built and run. That is the cost of an exact
executor size instead of the SystemModel's lower bound, and issue 0641's
provenance stamp means it is paid once per source change.

### What this changes about the phase-367 W4 retry

The retry loop that drops CMake-named components stays and is still right — it
is what makes ONE bad component cost its own sidecar rather than the
workspace's. It simply no longer fires here, because nothing is bad any more.
Its acceptance is the unit tests, not this workspace.
