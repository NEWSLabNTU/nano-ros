# Phase 209 — C++ port friction reduction

**Scope.** nano-ros targets **ROS 2 broadly**, not any one user (Autoware,
PX4, …). This phase is about the **generic** rclcpp/ament-cmake friction that
stops a typical small ROS 2 C++ node from compiling against nano-ros with just
build-script changes. Autoware appears below only as a real-world *measurement*
target (the survey
[`docs/research/autoware-port-survey.md`](../research/autoware-port-survey.md)
picked three small ROS 2 nodes from it). Project-specific helper libraries
(autoware's `universe_utils`/`vehicle_info_utils`, PX4's uORB shims, …) are
downstream — the porting user / their project carries them. nano-ros ships only
ROS-2-generic shims.

**Goal.** Make a normal ROS 2 C++ node land in nano-ros by **swapping the build
glue + one or two `#include`s**, not by rewriting the source. The three survey
candidates (`autoware_external_cmd_selector`, `topic_state_monitor`,
`autoware_steer_offset_estimator`) are concrete fixtures to validate against.

**Status.** Proposed (2026-05-30). Not started — design + scoping.

**Priority.** P2 — adoption-path work, not a capability gap. Existing users with
Rust-rewrite ports (Sentinel) are unaffected.

**Depends on.** Nothing blocking; orthogonal to the embedded-size phase 204 and
the BYO Zephyr starter 205.A. **Unblocked by:** `nros-cpp` (already mirrors
rclcpp 0.7.0 for pub/sub/service/client/action/parameter) + `nros generate cpp`
(already produces C++ headers from `.msg`).

## Overview — what blocks a "swap build scripts + minor change" port today

Three layers of friction (ranked by how often they bite), all generic to ROS 2
C++ nodes — not specific to any user project:

1. **The CMake glue.** A typical ROS 2 `CMakeLists.txt` calls
   `find_package(ament_cmake_auto REQUIRED)` + `ament_auto_find_build_dependencies()`
   + `ament_auto_add_library(... SHARED)` + `rclcpp_components_register_node(...)`
   + `ament_auto_package(INSTALL_TO_SHARE config launch)`. None of those exist on
   the nano-ros side; today every port writes its own `add_subdirectory(<nano-ros>) +
   target_link_libraries(... NanoRos::NanoRos)`. That's *one block to delete + one
   to add*, but it's per-package boilerplate that a single `NrosRclcppCompat`
   cmake module can collapse to one `include()` call.

2. **The header path.** `#include <rclcpp/rclcpp.hpp>` → `#include <nros/nros.hpp>`
   and `rclcpp::Node` → `nros::Node`. Today that's a per-file find/replace; an
   `nros/rclcpp_compat.hpp` header (alias namespace + alias the `rclcpp` types
   `nros-cpp` already mirrors) lets the original sources compile with **only
   the include swapped**. Tiny header, big DX win.

3. **`diagnostic_updater`.** A stock ROS 2 package (lives in
   `ros2/diagnostics`, not in any vehicle project); almost every C++ ROS 2 node
   touches it because it publishes `diagnostic_msgs/DiagnosticArray` — the
   standard ROS 2 health surface. Doesn't ship against nano-ros today. A small
   compat shim (~200 LOC) unblocks every node using it. **Project-specific
   helpers (e.g. `autoware_universe_utils`, PX4-shims) are not nano-ros's to
   ship** — the porting user vendors them or replaces the call sites with raw
   `nros-cpp` ones on a per-project basis.

Two additional, smaller ROS-2-generic friction sources:

4. **`rclcpp_components::RCLCPP_COMPONENTS_REGISTER_NODE` macro.** A stock ROS 2
   pattern — every node ends with this so a `ComponentManager` can compose it at
   runtime. On a single-binary embedded target the macro is meaningless (the
   binary has one node, statically wired); a header that `#define`s it to
   nothing makes the source compile.

5. **Yaml-loaded parameters.** `declare_parameter<double>("name", default)`
   calls resolve from a launch-time yaml in stock ROS 2. nano-ros embedded has
   no yaml loader. For small nodes the params are few; a build-time bake
   (`nros bake-params <file>.yaml -o params.hpp` emitting a constexpr table)
   keeps the source untouched.

## Work Items (ranked by impact / smallest-first)

### 209.A — `nros/rclcpp_compat.hpp` source-compat header
- [x] **Shipped (2026-05-30, branch `phase-209-cpp-port-friction-reduction`).**
      `packages/core/nros-cpp/include/nros/rclcpp_compat.hpp` lands the surface
      below plus a `rclcpp::Node` shim wrapping `nros::Executor` + `nros::Node`
      so the rclcpp idiom `std::make_shared<rclcpp::Node>("n")` →
      `n->create_publisher<M>(topic, qos)` (shared_ptr-returning) → `rclcpp::spin(n)`
      compiles unchanged. Also: `rclcpp_action::Server/Client` aliases, log
      macros (`RCLCPP_INFO/WARN/ERROR/DEBUG/FATAL` + `_THROTTLE` degrading to
      plain log), `rclcpp::init/shutdown/ok/spin_some`, `Logger`/`get_logger`,
      `QoS` factories. Scope + out-of-scope (NodeOptions parameter declare,
      tf2, lifecycle, callback groups) listed in the header comment. The
      alias-only sketch below remains a useful summary:
- [ ] Original sketch (kept for reference):
      ```cpp
      namespace rclcpp {
        using Node          = nros::Node;
        template <class M> using Publisher    = nros::Publisher<M>;
        template <class M> using Subscription = nros::Subscription<M>;
        template <class M> using Service      = nros::Service<M>;
        template <class M> using Client       = nros::Client<M>;
        using QoS           = nros::QoS;
        using Time          = nros::Time;
        using Duration      = nros::Duration;
        inline void spin(nros::Node::SharedPtr node) { nros::spin(std::move(node)); }
        inline int init(int argc, char ** argv) { return nros::init(argc, argv); }
        inline int shutdown() { return nros::shutdown(); }
      }
      ```
      Plus the `std::make_shared`-style `Node::make_shared(...)` factory shape.
      The header `#include`s `<nros/nros.hpp>`, so a node's source needs only
      `#include <nros/rclcpp_compat.hpp>` instead of `<rclcpp/rclcpp.hpp>` —
      every `rclcpp::Node` / `rclcpp::Publisher<M>` in the body resolves through
      the alias. **Size:** ~80 LOC, header-only.
- [ ] **Acceptance:** an Autoware Tier-1 source file compiles unchanged (apart
      from the include swap) against this header. (Full Autoware compile is
      209.G work; 209.A's header lands the surface 209.G will exercise. A
      minimal cmake-built smoke fits naturally into 209.B's
      `find_package(rclcpp)` shim.)

### 209.B — `NrosRclcppCompat` cmake module
- [ ] Add `cmake/compat/NrosRclcppCompat.cmake` that maps the stock
      `ament_cmake_auto` / `rclcpp_components` pattern to the nano-ros
      consumption shape:
      - `find_package(ament_cmake_auto REQUIRED)` → no-op (already loaded).
      - `ament_auto_find_build_dependencies()` → no-op (deps come from
        `target_link_libraries(NanoRos::NanoRos)`).
      - `ament_auto_add_library(<name> SHARED src/*.cpp)` → `add_library(<name>
        STATIC ...) + nros_platform_link_app(<name>)`.
      - `rclcpp_components_register_node(<name> PLUGIN <class> EXECUTABLE <bin>)`
        → emit a thin `int main()` that constructs the registered class +
        `nros::spin`s it. (Single-binary embedded; no runtime composition.)
      - `ament_auto_package(INSTALL_TO_SHARE …)` → no-op.
      The original `CMakeLists.txt` then needs only **one new `include()` at the
      top** instead of a rewrite. **Size:** ~150 LOC cmake.
- [ ] **Acceptance:** an unmodified ROS 2 `CMakeLists.txt` builds against
      nano-ros after `include(NrosRclcppCompat)` is prepended.

### 209.C — `RCLCPP_COMPONENTS_REGISTER_NODE` no-op shim
- [ ] Header `packages/core/nros-cpp/include/nros/rclcpp_components_compat.hpp`
      defining `RCLCPP_COMPONENTS_REGISTER_NODE(class) /* no-op */` so source
      lines using the macro compile. (The cmake `rclcpp_components_register_node`
      from 209.B emits the `main` instead.) **Size:** ~10 LOC.
- [ ] **Acceptance:** sources with the macro compile through nano-ros without
      modification.

### 209.D — `nros-diagnostic-updater` C++ shim crate
- [ ] New crate `packages/core/nros-diagnostic-updater/` exposing the
      `diagnostic_updater::Updater` surface used by Autoware nodes:
      - `Updater(nros::Node*, double frequency_hz)`.
      - `add(name, std::function<void(DiagnosticStatusWrapper &)>)`.
      - `setHardwareID(id)`.
      - `force_update()`.
      Internally: a periodic timer publishes `diagnostic_msgs/DiagnosticArray`
      with each registered task's `DiagnosticStatusWrapper` filled by its
      callback. `DiagnosticStatusWrapper` is a header-only typed view over
      `diagnostic_msgs/DiagnosticStatus`. The codegen for `diagnostic_msgs` lives
      in the bundled base interfaces. **Size:** ~200 LOC C++ + tests.
- [ ] **Acceptance:** an Autoware node using `diagnostic_updater::Updater` +
      `setHardwareID` + a single `add(...)` task compiles + publishes
      `DiagnosticArray` on the configured topic in a nano-ros build.

### 209.E — `nros generate cpp --workspace <ws>` ROS 2 msg bulk codegen
- [ ] Today `nros generate cpp <pkg>` runs per package. A real-world ROS 2 port
      transitively needs 5–20 message packages (the stock `geometry_msgs` /
      `nav_msgs` / `diagnostic_msgs` / `tf2_msgs` / `sensor_msgs` set + whatever
      the project ships). Add a `--workspace <path>` (or `--scan <path>`) shape
      that crawls every `package.xml` under the path with
      `<build_type>ament_cmake</build_type>` + a `msg/*.msg` directory and runs
      codegen for each, respecting the per-package `<depend>` graph. ROS-2-generic;
      any colcon workspace. (nros-cli work — owned in the standalone repo.)
- [ ] **Acceptance:** `nros generate cpp --workspace <a-ros2-workspace>`
      produces compiling headers for every msg package the surveyed nodes
      transitively need.

### 209.F — `nros bake-params <file>.yaml -o params.hpp`
- [ ] A user passes the original Autoware node yaml + a path; nros emits a
      header with `static constexpr` values keyed by parameter name. Wire
      `declare_parameter<T>(name, default)` in `nros-cpp` to look the name up in
      that header at compile time (or via a registered constexpr table). The
      source change is then **none** — the original `declare_parameter` call
      lands the baked value. **Size:** medium (~300 LOC nros-cli + ~50 LOC
      nros-cpp glue).
- [ ] **Acceptance:** a Tier-1 node's original yaml + source produce a working
      embedded binary with the bake step in the build.

### 209.G — Walking the first port end-to-end (the proof, + a book page)
- [ ] Pick a small real-world ROS 2 node (the survey nominates
      `topic_state_monitor` from `~/repos/autoware_universe/system/` as the
      smallest + most generic). Copy it under `examples/templates/cpp-port-<name>/`;
      show it building with 209.A + 209.B + 209.C + 209.D shipped, the original
      source files essentially untouched. Land the example + a book page
      (`book/src/getting-started/porting-a-cpp-node.md`) that walks the diff.
- [ ] **Acceptance:** the example compiles + boots on `native_sim` (Zephyr) and
      publishes `DiagnosticArray`; the book page is a copy-paste-able guide.

### 209.H — `rclcpp_lifecycle::LifecycleNode` mirror (deferred, P3)
- [ ] Stock ROS 2 nodes increasingly inherit
      `rclcpp_lifecycle::LifecycleNode` (REP-2002). nano-ros doesn't ship one.
      Add `nros::LifecycleNode` mirroring the transitions (`configure → activate
      → deactivate → cleanup`). Substantial work; deferred.

### Out-of-scope — project-specific helper libraries

Nodes from a specific ROS 2 project (Autoware's `universe_utils` /
`vehicle_info_utils`, PX4's uORB shims, navigation2's `nav2_util`, …) carry
their own helper libraries. **Those are downstream concerns — the porting user
vendors them or rewrites the call sites against raw `nros-cpp`.** nano-ros
ships only ROS-2-generic shims (the items above). A Tier-1 port that touches
such a helper either pulls the helper's source into the example tree as
vendored code or substitutes raw `nros-cpp` calls; either way, the
nano-ros-side surface stays generic.

## Sequencing

A **minimum viable port** ships when 209.A + 209.B + 209.C + 209.D are landed
(≈ a week of work). That's the "swap the build scripts + a `#include` + include
the cmake compat" promise made good for a small ROS 2 C++ node with no yaml
params. 209.E (workspace codegen) is concurrent and lives in nros-cli. 209.F
(param bake) lifts the ceiling to small param-loaded nodes. 209.G is the proof.
209.H (LifecycleNode mirror) is a separate, larger piece.

## Notes

- The proof — 209.G — is **the real measurement**. Until a real ROS 2 C++ source
  builds against nano-ros with the shims in place, the friction estimate is a
  projection, not a fact.
- All items above are generic to ROS 2 C++ surfaces (rclcpp, ament_cmake,
  diagnostic_msgs). Specific user projects (Autoware, PX4, navigation2, …) are
  consumers of the result; their project-specific helper libraries are not
  nano-ros's to ship — the survey just picked them as concrete fixtures because
  they're realistic small nodes.
- The 209.E codegen change has the longest tail: real ROS 2 msg trees have
  cross-package `#include`s the per-package invocation of `nros generate cpp`
  doesn't resolve. The workspace shape is the smallest UX that makes a single
  `nros generate` call enough.
