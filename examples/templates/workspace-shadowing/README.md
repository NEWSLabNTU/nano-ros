# workspace-shadowing — Phase 210.F.4 fixture

Smoke fixture proving the **workspace-over-AMENT** interface-package
shadowing contract.

## What it does

A workspace `src/std_msgs/` (carrying a `Marker.msg` upstream ROS 2's
`std_msgs` does **not** ship) shadows the AMENT-installed `std_msgs`
under `/opt/ros/<distro>/share/std_msgs/`. The consumer in
`src/consumer/` calls plain `find_package(std_msgs REQUIRED)` and
references `std_msgs::msg::Marker`. The layered resolver
`cmake/compat/stubs/_NrosFindRosMsgPackage.cmake` walks:

1. **`NROS_INTERFACE_SEARCH_PATH`** — set to `${CMAKE_SOURCE_DIR}/src`
   by the umbrella `CMakeLists.txt`, so the workspace shadow wins.
2. `AMENT_PREFIX_PATH` — `/opt/ros/<distro>/share/std_msgs` is **NOT**
   reached. (If it were, the compile would FAIL — AMENT's std_msgs
   ships no `Marker.msg`.)
3. Bundled (`<nano-ros>/packages/interfaces/<pkg>/`).

A `message(STATUS nros: find_package(std_msgs) -> .../src/std_msgs)`
line is emitted at configure time.

## Reproducing the contract

```sh
# 1. Source ROS 2 so AMENT_PREFIX_PATH points at the upstream std_msgs.
source /opt/ros/humble/setup.bash

# 2. Build. No root build file (RFC-0098 D9) and no bringup, so `nros build`
#    builds each package into its own `build/<pkg>/`, generating the cmake
#    root the consumer needs (phase-445 W5).
cd examples/templates/workspace-shadowing
export NROS_REPO_DIR=/path/to/nano-ros
nros sync
nros build

# 3. Verify the workspace copy was linked (not AMENT's) — the two symbols
#    `tests/workspace_shadowing.rs` asserts. A field name such as
#    `shadowed_marker` leaves no symbol of its own, so grep for the type.
nm -C build/consumer/cmake/pkg/consumer/consumer | grep -E 'nros_cpp_serialize_std_msgs_msg_marker|std_msgs::msg::Marker'
# >>> ... T nros_cpp_serialize_std_msgs_msg_marker
# >>> ... std_msgs::msg::Marker ...
```

If `find_package(std_msgs)` had resolved to AMENT, step 2 would fail
at compile time on `#include "std_msgs/msg/marker.hpp"` — AMENT's
`std_msgs` carries no `Marker.msg`. The successful build is the
shadowing proof; the `nm` grep is the symbol-level corroborator.

## Symbol-table verification

The consumer binary's symbol closure contains the workspace-shadowed
`std_msgs::msg::Marker` type. `shadowed_marker` is the unique field
name — it appears nowhere in upstream ROS 2's `std_msgs`. A grep for
`shadowed_marker` in the consumer binary's `nm` output is therefore
direct, unambiguous evidence that the workspace copy supplied the
type.

## Layered search path

| Priority | Layer | Source |
|---|---|---|
| 1 (highest) | `NROS_INTERFACE_SEARCH_PATH` | Colon/semicolon-separated colcon-`src/`-style roots |
| 2 | `AMENT_PREFIX_PATH` | Upstream ROS install (`<prefix>/share/<pkg>/`) |
| 3 (lowest) | Bundled | `<nano-ros>/packages/interfaces/<pkg>/` |

When two layers carry the same pkg name, the higher layer wins. This
fixture is the smoke proof for the Layer 1 > Layer 2 case.

## Cross-references

* Phase doc — `docs/roadmap/phase-210-ros-convention-codegen.md`
  §210.F.4.
* Layered resolver — `cmake/compat/stubs/_NrosFindRosMsgPackage.cmake`.
* Bulk codegen orchestrator — `cmake/NanoRosGenerateInterfaces.cmake`'s
  `nros_workspace_interfaces()` (handles intra-workspace shadowing).
* Book — `book/src/getting-started/your-own-msg-package.md` §Shadowing
  contract.
* Regression test —
  `packages/testing/nros-tests/tests/phase210_f4_shadowing.rs`.
* Sibling fixture — `examples/templates/local-msg-package/` (210.A.4
  / F.1 mixed-workspace shape, no shadowing).
