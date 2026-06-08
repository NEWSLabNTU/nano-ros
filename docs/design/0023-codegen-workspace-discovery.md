---
rfc: 0023
title: "Codegen — workspace / AMENT discovery — design + revision plan"
status: Stable
since: 2026-05
last-reviewed: 2026-05
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# Codegen — workspace / AMENT discovery — design + revision plan

**Goal.** A msg package authored against the ROS 2 convention (verbatim
`package.xml` + `msg/*.msg` + the standard `rosidl_generate_interfaces(...)`
CMakeLists block) builds **unmodified** against nano-ros, whether it lives
in the user's local workspace (`src/<pkg>/`) or in an ament-installed prefix
on `AMENT_PREFIX_PATH`. Existing ROS msg source packages drop in as-is; we
roll our own codegen but the SOURCE LAYOUT and the CMake CALL SHAPE are
ROS's, so the same `src/` works under both `colcon build` (which generates
its own bindings) and a nano-ros build (which generates ours).

## ROS conventions we adopt verbatim

### Msg package source layout (unchanged from ROS)

```
my_msgs/
├── package.xml           # standard: <buildtool_depend>rosidl_default_generators</buildtool_depend>
│                         #           <member_of_group>rosidl_interface_packages</member_of_group>
│                         #           <depend>std_msgs</depend>
├── CMakeLists.txt        # standard: rosidl_generate_interfaces(${PROJECT_NAME}
│                         #             "msg/MyMsg.msg" DEPENDENCIES std_msgs)
├── msg/MyMsg.msg
├── srv/MySrv.srv
└── action/MyAct.action
```

No nano-ros-specific files in a msg package. The pkg is identical to what
`colcon build` consumes.

### CMake function names + signatures (matching `rosidl_default_generators`)

`rosidl_generate_interfaces` (the canonical upstream function) becomes our
public surface — nano-ros ships a function with the SAME name + signature,
which internally drives nano-ros codegen. The user's
`my_msgs/CMakeLists.txt` is unmodified ROS:

```cmake
cmake_minimum_required(VERSION 3.8)
project(my_msgs)

find_package(ament_cmake REQUIRED)            # nano-ros: no-op stub
find_package(rosidl_default_generators REQUIRED)  # nano-ros: no-op stub
find_package(std_msgs REQUIRED)               # nano-ros: smart stub — see below

rosidl_generate_interfaces(${PROJECT_NAME}    # nano-ros: provides this fn
  "msg/MyMsg.msg"
  "srv/MySrv.srv"
  DEPENDENCIES std_msgs
)

ament_export_dependencies(rosidl_default_runtime)  # nano-ros: no-op stub
ament_package()                                    # nano-ros: no-op stub
```

That CMakeLists builds under colcon (rosidl_default_generators emits the
upstream typesupport libs) AND under nano-ros (our `rosidl_generate_interfaces`
implementation emits `${PROJECT_NAME}__nano_ros_cpp` and the `nano_ros_rust`
crate, same shape as today's `nros_generate_interfaces` but driven from the
explicit file list the upstream signature demands).

### Consuming a sourced ROS msg pkg

```cmake
find_package(std_msgs REQUIRED)
```

Stock ROS: this finds `std_msgs`'s ament `<prefix>/share/std_msgs/` dir and
loads its `<prefix>/lib/cmake/std_msgs/std_msgsConfig.cmake`, which sets up
the colcon-built typesupport targets.

nano-ros: a **smart Find-stub** does the same lookup (via
`AMENT_PREFIX_PATH/share/std_msgs/`), but instead of loading colcon's
typesupport it:

1. Reads `share/std_msgs/package.xml` (deps).
2. Globs `share/std_msgs/{msg,srv,action}/*` (interface files).
3. Calls nano-ros codegen on them, emitting `std_msgs__nano_ros_cpp` etc.
4. Creates IMPORTED INTERFACE targets `std_msgs::std_msgs` aliasing the
   generated libs, so `target_link_libraries(... std_msgs::std_msgs)`
   resolves to nano-ros's bindings.

The user's CMakeLists is unchanged from the colcon shape.

## Interface-package search path (uniform across workspace + AMENT)

A single layered search resolves a pkg name through (in priority order):

1. **`NROS_INTERFACE_SEARCH_PATH`** — colon-separated dirs (env or
   `-DNROS_INTERFACE_SEARCH_PATH=...`). Each entry treated as a colcon-style
   `src/` root: every immediate subdir with a `package.xml` is a candidate.
2. **`AMENT_PREFIX_PATH`** — every `<prefix>/share/<pkg>/package.xml`
   already-installed via `colcon build && source install/setup.bash` or via
   `apt install ros-humble-<pkg>`.
3. **`<nano-ros>/packages/interfaces/`** — bundled in-tree fallback (today's
   `rcl-interfaces`, `lifecycle-msgs`).

The smart Find-stub for any pkg walks this path. When a name matches in
multiple layers, the higher-priority wins (workspace > AMENT > bundled);
shadowing emits a warning.

## Public surface — exact ROS spelling

### CMake — what users call

```cmake
# Identical to upstream rosidl_default_generators:
rosidl_generate_interfaces(<target>
    <files>...
    [DEPENDENCIES <pkg>...]
    [SKIP_INSTALL]
    [LIBRARY_NAME <name>]
    [ADD_LINTER_TESTS]                # accepted, no-op (no rosidl linters)
    [SKIP_GROUP_MEMBERSHIP_CHECK]     # accepted, no-op
)

# Identical to upstream find_package against an ament install:
find_package(<pkg> REQUIRED)         # smart stub: AMENT/workspace lookup +
                                     # nano-ros codegen + IMPORTED targets

# nano-ros convenience (NOT in ROS — explicit opt-in for the bulk pattern):
nros_workspace_interfaces([PATHS <dir>...] [LANGUAGE CPP|RUST])
    # Scans the search path, finds every package.xml whose <member_of_group>
    # is rosidl_interface_packages, topo-sorts, add_subdirectory each.
    # Equivalent to writing one add_subdirectory(src/<pkg>) per pkg by hand.
```

The legacy `nros_generate_interfaces(<pkg> ...)` + `nros_find_interfaces()`
keep working but are documented as **deprecated** in favour of
`rosidl_generate_interfaces` (the upstream call shape). The
`nros_*` functions become thin wrappers internally; user-facing docs say
"use the standard rosidl shape."

### CLI (`nros codegen`)

```bash
# Single-package (today's per-pkg form, unchanged):
nros generate cpp <pkg>
nros generate-rust            # in a pkg dir; reads ./package.xml

# Bulk (matches `colcon build --packages-select` semantics):
nros generate cpp --workspace <dir> [--target <pkg>...] [--out <dir>]
nros generate-rust --workspace <dir> [--target <pkg>...] [--out <dir>]
```

`--workspace <dir>` walks `<dir>` for `package.xml`s where
`<member_of_group>rosidl_interface_packages</member_of_group>` — same
discovery the cmake helper does.

### Env

```bash
source /opt/ros/humble/setup.bash                          # AMENT_PREFIX_PATH
export NROS_INTERFACE_SEARCH_PATH=$HOME/my_ws/src          # local workspace

cmake -B build -S .            # finds my_msgs alongside sourced std_msgs
```

Optional `nros workspace env` CLI prints the export line for a given
workspace dir.

## What changes per layer

### `cmake/compat/NrosRclcppCompat.cmake` + Find-stubs

- **Smart Find-stub generator** at `cmake/compat/stubs/_NrosFindRosMsgPackage.cmake`:
  walks the search path, finds the named pkg, runs nano-ros codegen on its
  interface files, emits the canonical `${pkg}::${pkg}` IMPORTED INTERFACE
  + the per-language `${pkg}__nano_ros_cpp` / `__nano_ros_rust` targets.
- Per-pkg `cmake/compat/stubs/Find<msg-pkg>.cmake` becomes a 2-liner:
  ```cmake
  include(${CMAKE_CURRENT_LIST_DIR}/_NrosFindRosMsgPackage.cmake)
  _nros_find_ros_msg_package(<pkg>)
  ```
  Same for every msg pkg (std_msgs, sensor_msgs, geometry_msgs, ...).
  Adding a new one is one file ≈ two lines.

### `cmake/NanoRosGenerateInterfaces.cmake`

- Add `rosidl_generate_interfaces(<target> <files>... [DEPENDENCIES ...]
  [SKIP_INSTALL] [LIBRARY_NAME ...] [ADD_LINTER_TESTS]
  [SKIP_GROUP_MEMBERSHIP_CHECK])`. The function takes explicit file paths
  (upstream shape) — internally calls the same codegen `nros_generate_interfaces`
  uses, but bypasses the name-lookup search path (files are explicit).
  Honours every upstream flag (the rosidl-specific ones, e.g. linter tests,
  become no-ops with a `message(STATUS …)` note).
- Existing `nros_generate_interfaces(<pkg>)` becomes a thin wrapper that
  resolves the pkg via the new search path, then forwards file paths to
  `rosidl_generate_interfaces`.
- Add `nros_workspace_interfaces([PATHS <dir>...] [LANGUAGE …])`:
  scans the search path, identifies msg packages by `<member_of_group>
  rosidl_interface_packages</member_of_group>` in their package.xml, topo-
  sorts, calls `add_subdirectory(<pkg-dir>)` so each pkg's own CMakeLists
  runs (which calls `rosidl_generate_interfaces`).

### `nros codegen` (lives in nros-cli)

- New `--workspace <dir>` flag on `nros codegen resolve-deps`,
  `nros generate cpp`, `nros generate-rust`.
- Honours `NROS_INTERFACE_SEARCH_PATH` env.
- Also emits the upstream-style `<pkg>/msg/<name>.hpp` header alongside the
  current `<pkg>/<pkg>.hpp` umbrella (closes the 209.G-iter-2 codegen
  cosmetic).

### Rust path

- `packages/core/nros-build-codegen` build-time helper crate. A consuming
  crate's `build.rs`:
  ```rust
  fn main() {
      nros_build_codegen::workspace().run().unwrap();
  }
  ```
  Discovers + codegens via the same search path, same result shape.

## Revision plan (each stage independent)

### Stage A — cmake compat: `rosidl_generate_interfaces` + smart Find-stub

1. Implement `rosidl_generate_interfaces(<target> <files> ...)` in
   `cmake/NanoRosGenerateInterfaces.cmake`. Wires to existing codegen.
2. Write `_NrosFindRosMsgPackage.cmake` helper (AMENT + workspace search
   path → codegen → IMPORTED INTERFACE target).
3. Reduce `cmake/compat/stubs/Find<msg-pkg>.cmake` files to the 2-line
   include + delegate. Add a unit-test fixture proving a tiny user
   `src/my_msgs/` (verbatim ROS `package.xml + msg/MyMsg.msg + ROS-shape
   CMakeLists.txt`) builds + links from a consumer that just writes
   `find_package(my_msgs REQUIRED)` + `target_link_libraries(my_node
   my_msgs::my_msgs)`.

### Stage B — `nros_workspace_interfaces()` + env

1. Implement `nros_workspace_interfaces([PATHS ...] [LANGUAGE …])` as the
   bulk-`add_subdirectory` orchestrator.
2. `NROS_INTERFACE_SEARCH_PATH` env + cmake var plumbed through the
   smart Find-stub + the workspace fn.

### Stage C — `nros codegen --workspace` (in nros-cli)

1. CLI flag.
2. Topo-walks + emits everything. Optional `--out <dir>` for separate-build
   layouts (not strictly needed when the cmake path drives codegen via
   `rosidl_generate_interfaces`).
3. Upstream-style `<pkg>/msg/<name>.hpp` header layout (also closes 209.E).

### Stage D — Rust build.rs helper

1. `nros-build-codegen` crate.
2. Convert one rust example.

### Stage E — UX + docs + migration

1. Book page: `getting-started/your-own-msg-package.md` — "write a normal ROS
   msg pkg, drop it into `src/`, build."
2. Migrate in-tree per-pkg `nros_generate_interfaces(<pkg>)` call sites to
   the `rosidl_generate_interfaces` shape (those that author msgs) or
   `find_package(<pkg>)` (those that consume msgs).
3. `nros workspace env` CLI subcommand (mirrors `colcon`'s setup.bash
   ergonomics).

## Acceptance

- A standard ROS msg package — package.xml with
  `<member_of_group>rosidl_interface_packages</member_of_group>`, CMakeLists
  with `rosidl_generate_interfaces(${PROJECT_NAME} "msg/X.msg" DEPENDENCIES
  std_msgs)` — builds against nano-ros **with the same source tree** as
  under `colcon build`. The pkg's CMakeLists.txt contains zero
  nano-ros-specific lines.
- A consumer node writes `find_package(my_msgs REQUIRED)` +
  `target_link_libraries(my_node my_msgs::my_msgs)` (exact upstream shape) —
  no `nros_generate_interfaces(...)` call.
- An app's `CMakeLists.txt` drops to one boilerplate-collapse helper call —
  `nros_workspace_interfaces()` — instead of N per-pkg codegen lines, when
  the user wants to opt into bulk discovery.
- The same `src/` workspace builds with both `colcon build` and a nano-ros
  cmake build — the source layout + CMakeLists shape are identical.

## Compat / risks

- Per-pkg `nros_generate_interfaces(<pkg>)` keeps working (legacy alias).
  Marked deprecated in docs; in-tree migration happens incrementally.
- The smart Find-stub generator runs codegen at cmake-configure time. If a
  configure is invoked from a no-AMENT, no-NROS_INTERFACE_SEARCH_PATH shell
  and the pkg lives nowhere reachable, `find_package(... REQUIRED)` fails
  with a clear "looked in <list>" error.
- Shadowing across layers (workspace's `std_msgs` shadows AMENT's
  `std_msgs`) → warn loudly + take workspace.
