---
rfc: 0048
title: "Ament-aligned CMake consumption — source-backed find_package, one shape per platform"
status: Draft
since: 2026-07
last-reviewed: 2026-07
implements-tracked-by: [phase-287]
supersedes: []
superseded-by: null
---

# RFC-0048 — Ament-aligned CMake consumption

## Summary

A nano-ros C/C++ package is written in the **ament_cmake convention** a ROS 2
developer already knows — `find_package(nano_ros REQUIRED)`,
`find_package(<msg_pkg>)`, an `add_*` verb, `ament_target_dependencies`,
`install`, `ament_package` — and its `CMakeLists.txt` is **byte-identical across
every platform** (native, FreeRTOS, NuttX, ThreadX, Zephyr). The per-package
delta (which board, which RMW) lives in `package.xml`'s `<export>`, ament's own
extension point. Resolution is **source-backed, not install-backed** (nano-ros
is a source distribution, #171 D2): `find_package(nano_ros)` locates the pulled
checkout via `nano_ros_ROOT` + an in-tree config package — no crates.io, no
`install()` prefix, no `add_subdirectory(<repo-root>)` boilerplate.

Implements #171 decision **D5**; consumes phase-288 (#171 D1/D2 source
bootstrap). Builds on RFC-0018 (nros-cpp), RFC-0019 (nros-c), RFC-0014 (`nros
setup`), RFC-0023 (ament codegen discovery), RFC-0026 (examples layout).

## Motivation

The pre-2026-07 shape (verified across 233 example `CMakeLists.txt`):

- A ~10-line `NANO_ROS_ROOT` resolve guard copy-pasted into every leaf, **drifted
  24–34 lines apart** — the worst state for scaffolding.
- A per-leaf `if(NROS_RMW STREQUAL "cyclonedds") enable_language(CXX) endif()`
  **micro-option** the user had to get right.
- Embedded leaves additionally open with `set(NANO_ROS_PLATFORM …)` +
  `set(NANO_ROS_BOARD …)` + a `nano_ros_deploy()` call — a **different shape**
  from native (which used `nano_ros_entry` + a hand-named `target_link_libraries(<pkg>__nano_ros_<lang>)`).
- Hand-naming the generated msg libs (`std_msgs__nano_ros_c`).

Net: three shapes (native / embedded / zephyr), drift, and knobs a user should
never own. A ROS 2 porter recognises none of it.

## Design

### 1. Source-backed `find_package(nano_ros)`

nano-ros ships an in-tree `nano_rosConfig.cmake`. `find_package(nano_ros
REQUIRED)` locates it via **`nano_ros_ROOT`** (the canonical per-package hint,
CMake ≥3.12), which `activate.sh` exports (`= $NROS_REPO_DIR`); copy-out without
activate passes `-Dnano_ros_ROOT=<checkout>` (or the CMakePreset carries it, §6).
No install, no `AMENT_PREFIX_PATH`, no `find_package(NanoRos)` config on a
system prefix (the Phase-140 install rules stay retired). The config:

- runs the workspace import (`add_subdirectory(<checkout>)` once, idempotent) —
  the machinery phase-287 W1 landed as `nano_ros_bootstrap()`, now internal to
  the config;
- enables CXX iff the resolved RMW needs it (the old micro-option, hidden);
- registers the msg-codegen redirect (§2);
- defines the `nano_ros_add_executable` / `nano_ros_add_node` verbs (§3).

### 2. `find_package(<msg_pkg>)` — the ament line, codegen from `package.xml`

Ament consumers `find_package(std_msgs)` to pick up a **pre-built** typesupport
lib. nano-ros has no install, so the bindings must be **codegen'd**.

**As implemented (phase-287 W3), `find_package(<msg_pkg>)` is a validate-only
line** — it resolves the package (via the compat find-stubs on `CMAKE_MODULE_PATH`)
to satisfy the ament `REQUIRED` and confirm the dependency, but it does **not**
itself generate. The authoritative codegen is driven by the `nano_ros_add_*` verb,
which knows the leaf's language (inferred from its sources) and reads the
`package.xml` `<depend>` closure through `nros codegen resolve-deps` (the CLI path
that resolves well-known ROS packages with **no in-tree bundle or sourced ROS
install** — the find-stub's cmake-glob resolution cannot). This keeps C and C++
leaves byte-identical and avoids pulling a C++ interface lib (and CXX
target-features) into a C leaf's scope from a `find_package` line.

> **Rationale for the split** (vs. the originally-sketched per-line
> `CMAKE_FIND_PACKAGE_REDIRECTS_DIR` mechanism, CMake ≥3.24): the redirect approach
> needs the find-stub to resolve each package's IDLs itself, which fails for
> well-known ROS packages that have no in-tree bundle. Driving codegen from
> `package.xml <depend>` via the CLI is both robust and more ament-idiomatic
> (deps are declared in the manifest). The floor stays
> **`cmake_minimum_required(VERSION 3.22)`** — resolution uses `nano_ros_ROOT`
> (CMake ≥3.12) + the module-path find-stubs (ancient), not the 3.24 redirect.

### 3. Two verbs for two roles

An RTOS node is not always an executable, so a single `add_executable` cannot be
uniform (verified: `NanoRosEntry.cmake` — native/FreeRTOS/NuttX/ThreadX emit
`add_executable`; **Zephyr emits `add_library` linked into Zephyr's own `app`**,
which owns `main`). Two verbs, matching the two roles, so users don't confuse
them:

- **`nano_ros_add_executable(<name> <sources…>)`** — a *standalone entry* (own
  `main` / self-bringup). Emits `add_executable` on native/FreeRTOS/NuttX/ThreadX
  and `add_library`-into-`app` on Zephyr; the platform choice comes from
  `package.xml` (§4), so the call is identical everywhere.
- **`nano_ros_add_node(<name> <sources…> CLASS <ns::Class>)`** — a *workspace
  component* (no own `main`; registered into a carrier ELF). Always a component
  library.

Both are followed by `ament_target_dependencies(<name> <msg_pkg>…)` — the
familiar verb — which links the generated `*__nano_ros_<lang>` bindings.

### 4. `package.xml` is the SSoT — deploy in `<export>`

The per-package platform delta lives where ament already expects package
metadata:

```xml
<depend>std_msgs</depend>
<export>
  <build_type>ament_cmake</build_type>
  <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/>
</export>
```

`find_package(nano_ros)` + the `nano_ros_add_*` verbs read the `<nano_ros>` tuple
from the invoking package's `package.xml`. `deploy="native"` needs no board.
This is what keeps the `CMakeLists.txt` byte-identical across platforms — only
`package.xml` differs, and only in the one `<nano_ros>` line.

### 5. Interface (msg) packages

A package that defines its own `.msg`/`.srv`/`.action` mirrors
`rosidl_generate_interfaces`:

```cmake
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)
nano_ros_generate_interfaces(${PROJECT_NAME}
    "msg/Reading.msg" "srv/SetMode.srv"
    DEPENDENCIES std_msgs)
ament_package()
```

### 6. Toolchain automation — CMakePresets

Cross-compile toolchains must be set **before `project()`**, but
`find_package(nano_ros)` runs after — so the toolchain cannot come from it.
**CMakePresets `toolchainFile`** is the CMake-native answer (applied before the
first `project()`), and `nros` generates the presets so the user never
hand-writes one:

- **`nros setup <board>`** — after provisioning the board's toolchain/SDK (RFC-0014),
  writes `~/.nros/presets/<board>.json`:
  ```json
  { "configurePresets": [{
      "name": "freertos-mps2-an385",
      "toolchainFile": "$env{NROS_REPO_DIR}/cmake/toolchain/arm-freertos-armcm3.cmake",
      "cacheVariables": { "nano_ros_ROOT": "$env{NROS_REPO_DIR}" }
  }]}
  ```
- **`nros init`** (new verb) — in the user's project, generates a
  `CMakePresets.json` that `"include"`s the `~/.nros/presets/*` fragments.
  Idempotent; re-run after `nros setup` of a new board.

Then `cmake --preset <board>` configures with the toolchain + `nano_ros_ROOT`
pre-`project()`; `find_package(nano_ros)` resolves. Native needs no toolchain —
its preset carries only `nano_ros_ROOT` (or a bare `cmake` works when
`activate.sh` set it).

## The end-to-end user workflow

```bash
git clone --branch v0.X.Y https://github.com/NEWSLabNTU/nano-ros   # pinned source (D2)
./nano-ros/bootstrap.sh                 # builds nros CLI (phase-288)
source nano-ros/activate.sh             # PATH + NROS_REPO_DIR + nano_ros_ROOT
nros setup freertos-mps2-an385          # toolchain/SDK + ~/.nros/presets/<board>.json
# in your project:
nros init                               # CMakePresets.json wired to nano-ros
cmake --preset freertos-mps2-an385 && cmake --build --preset freertos-mps2-an385
```

## Old paths removed (by phase-287)

- The `NANO_ROS_ROOT` resolve guard block in every leaf → `find_package(nano_ros)`.
- `nano_ros_workspace_pkg_guard()` direct calls, `add_subdirectory(<repo-root>)`
  in leaves → internal to `nano_rosConfig.cmake`.
- `set(NANO_ROS_PLATFORM …)` / `set(NANO_ROS_BOARD …)` / `set(NROS_RMW … CACHE)` /
  `nano_ros_deploy(…)` CMake calls → `package.xml` `<export><nano_ros …/>`.
- Hand-named `target_link_libraries(<t> PRIVATE <pkg>__nano_ros_<lang>)` +
  `nros_platform_link_app(<t>)` → `ament_target_dependencies` + the verbs.
- The `if(NROS_RMW STREQUAL "cyclonedds") enable_language(CXX)` micro-option →
  hidden in `find_package(nano_ros)`.
- The phase-287 W1 interim `nano_ros_bootstrap()` / `nano_ros_link()`
  **user-facing** calls → superseded by `find_package(nano_ros)` + verbs (their
  logic survives as the config's internals). Native leaves migrated to
  `nano_ros_bootstrap` in W2a are re-migrated to the ament shape.

## Non-goals

- crates.io / prebuilt libraries (#171 D2) · PlatformIO / Arduino (#171 D3/D4).
- Reviving Phase-140 `install()` rules on a system prefix — resolution is
  source-backed via `nano_ros_ROOT`, not an install.
- Book prose about publish/future work (#171 D7).

## Open questions

- Whether `find_package(<msg_pkg>)` should also accept the ROS 2 versioned/`COMPONENTS`
  spelling, or only the bare package name.
- Whether `nros init` writes a project-local `CMakePresets.json` or a
  `CMakeUserPresets.json` (the latter is git-ignored by convention — better for a
  user's own repo).
