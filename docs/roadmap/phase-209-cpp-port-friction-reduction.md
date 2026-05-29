# Phase 209 — C++ port friction reduction

**Goal.** Make a normal ROS 2 C++ node — e.g. a small Autoware control node —
land in nano-ros by **swapping the build glue + one or two `#include`s**, not by
rewriting the source. The companion survey
([`docs/research/autoware-port-survey.md`](../research/autoware-port-survey.md))
identified three Tier-1 Autoware candidates (`autoware_external_cmd_selector`,
`topic_state_monitor`, `autoware_steer_offset_estimator`) that *could* fit this
shape; this phase fills the gaps that today stop them.

**Status.** Proposed (2026-05-30). Not started — design + scoping.

**Priority.** P2 — adoption-path work, not a capability gap. Existing users with
Rust-rewrite ports (Sentinel) are unaffected.

**Depends on.** Nothing blocking; orthogonal to the embedded-size phase 204 and
the BYO Zephyr starter 205.A. **Unblocked by:** `nros-cpp` (already mirrors
rclcpp 0.7.0 for pub/sub/service/client/action/parameter) + `nros generate cpp`
(already produces C++ headers from `.msg`).

## Overview — what blocks a "swap build scripts + minor change" port today

Three layers of friction (ranked by how often they bite):

1. **The CMake glue.** Autoware nodes' `CMakeLists.txt` calls
   `find_package(ament_cmake_auto REQUIRED)` + `ament_auto_find_build_dependencies()`
   + `ament_auto_add_library(... SHARED)` + `rclcpp_components_register_node(...)`
   + `ament_auto_package(INSTALL_TO_SHARE config launch)`. None of those exist on
   the nano-ros side; today every port writes its own `add_subdirectory(<nano-ros>) +
   target_link_libraries(... NanoRos::NanoRos)`. That's *one block to delete + one
   to add*, but it's per-package boilerplate that an `NrosAutowareCompat` cmake
   module can collapse to a single `include()` call.

2. **The header path.** `#include <rclcpp/rclcpp.hpp>` → `#include <nros/nros.hpp>`
   and `rclcpp::Node` → `nros::Node`. Today that's a per-file find/replace; an
   `nros/rclcpp_compat.hpp` header (alias namespace + alias the `rclcpp` types
   `nros-cpp` already mirrors) would let the original sources compile with **only
   the include swapped**. Tiny header, big DX win.

3. **Three pervasive helper libs.** Almost every Autoware node uses one or more
   of `diagnostic_updater`, `autoware_universe_utils`, `autoware_vehicle_info_utils`.
   None ships against nano-ros. The Tier-1 trio leans hardest on
   `diagnostic_updater`; the universe/vehicle utils are used more lightly and can
   be replaced with raw nros-cpp calls on first port. So **one new compat shim
   (`nros-diagnostic-updater`) unblocks the Tier-1 trio**; the heavier utility
   shims are needed only for Tier-2.

Two additional, smaller friction sources:

4. **`rclcpp_components::RCLCPP_COMPONENTS_REGISTER_NODE` macro.** Autoware nodes
   tail-end with this so a `ComponentManager` can compose them at runtime. On a
   single-binary embedded target the macro is meaningless (the binary has one
   node, statically wired); a header that `#define`s it to nothing makes the
   source compile.

5. **Yaml-loaded parameters.** `declare_parameter<double>("name", default)` calls
   resolve from a launch-time yaml in Autoware. nano-ros embedded has no yaml
   loader. For the Tier-1 trio the loaded params are few (4–8 floats); a build-
   time bake (`nros bake-params <file>.yaml -o params.hpp` emitting a constexpr
   table) keeps the source untouched.

## Work Items (ranked by impact / smallest-first)

### 209.A — `nros/rclcpp_compat.hpp` source-compat header
- [ ] Ship `packages/core/nros-cpp/include/nros/rclcpp_compat.hpp` that aliases
      the rclcpp surface `nros-cpp` already mirrors:
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
      from the include swap) against this header.

### 209.B — `NrosAutowareCompat` cmake module
- [ ] Add `cmake/compat/NrosAutowareCompat.cmake` that maps the autoware /
      `ament_cmake_auto` pattern to the nano-ros consumption shape:
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
- [ ] **Acceptance:** the unmodified Autoware Tier-1 `CMakeLists.txt` builds
      against nano-ros after `include(NrosAutowareCompat)` is prepended.

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

### 209.E — `nros generate cpp --workspace <ws>` autoware-msgs bulk codegen
- [ ] Today `nros generate cpp <pkg>` runs per package; an Autoware port needs
      it for `autoware_control_msgs`, `autoware_vehicle_msgs`,
      `autoware_system_msgs`, `autoware_adapi_v1_msgs`, `tier4_control_msgs`,
      `tier4_external_api_msgs`, `tier4_auto_msgs_converter`, `nav_msgs`,
      `geometry_msgs`, … typically 10–15 packages. Add a `--workspace <path>` (or
      `--scan <path>`) shape that crawls every `package.xml` under the path with
      `<build_type>ament_cmake</build_type>` + a `msg/*.msg` directory and runs
      codegen for each, respecting per-package `<depend>` graph. (nros-cli work
      — owned in the standalone repo.)
- [ ] **Acceptance:** `nros generate cpp --workspace ~/repos/autoware_universe`
      produces compiling headers for every msg package the Tier-1 trio
      transitively needs.

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

### 209.G — Tier-2 helper-lib shims (`universe_utils`, `vehicle_info_utils`)
- [ ] **`nros-cpp-universe-utils`** — the autoware utility surface
      (`PollingSubscriber<M>`, `DebugPublisher`, `Timer` thin wrapper,
      `createQuaternionFromYaw`, `calcDistance2d`, …). Header-only; ~400 LOC.
      Sentinel already has a Rust port of the *logic*; this is the C++ surface a
      Tier-2 Autoware port `#include`s.
- [ ] **`nros-cpp-vehicle-info-utils`** — a `VehicleInfo` struct + a `createVehicleInfo`
      factory that pulls constants from the bake-params (209.F) instead of the
      ROS parameter server.
- [ ] **Acceptance:** a Tier-2 Autoware node (e.g. `autoware_joy_controller` or
      `autoware_external_cmd_converter`) ports.

### 209.H — Walking the first port end-to-end (the proof, + a book page)
- [ ] Pick `topic_state_monitor` from `~/repos/autoware_universe/system/`; copy
      it under `examples/templates/autoware-port-<name>/`; show it building with
      209.A + 209.B + 209.C + 209.D shipped, the original source files
      essentially untouched. Land the example + a book page
      (`book/src/getting-started/porting-a-cpp-node.md`) that walks the diff.
- [ ] **Acceptance:** the example compiles + boots on `native_sim` (Zephyr) and
      publishes `DiagnosticArray`; the book page is a copy-paste-able guide.

### 209.I — `rclcpp_lifecycle::LifecycleNode` mirror (deferred, P3)
- [ ] Several Autoware system nodes (`hazard_status_converter`, parts of
      `default_ad_api`, …) inherit `rclcpp_lifecycle::LifecycleNode`. nano-ros
      doesn't ship one. Add `nros::LifecycleNode` mirroring REP-2002 transitions
      (`configure → activate → deactivate → cleanup`). Substantial work;
      deferred.

## Sequencing

A **minimum viable Tier-1 port** ships when 209.A + 209.B + 209.C + 209.D are
landed (≈ a week of work). That's the "swap the build scripts + a `#include` +
include the cmake compat" promise made good for control-only nodes that don't
load yaml params. 209.E (workspace codegen) is concurrent and lives in
nros-cli. 209.F (param bake) and 209.G (universe/vehicle-info shims) lift the
ceiling to ≈ a third of the Autoware control nodes. 209.I is a separate, larger
piece.

## Notes

- The proof — 209.H — is **the real measurement**. Until a real Autoware source
  builds against nano-ros with the shims in place, the friction estimate is a
  projection, not a fact. The Sentinel team chose Rust rewrite specifically
  because the C++-retain path hadn't been validated; this phase exists to
  validate it.
- Nothing here changes the Sentinel ports (they stay Rust). Tier-1 is **net-new
  ports of Autoware nodes Sentinel did not touch.** If Tier-1 fits embedded
  cleanly the cluster is "safety-island extras" (cmd-source arbitration, topic
  liveness, light filter); if not, Tier-2/3 are unreachable via the C++ path.
- The 209.E codegen change has the longest tail: an Autoware msg package
  graph (`autoware_*` + `tier4_*`) has cross-package `#include`s the per-package
  invocation of `nros generate cpp` doesn't resolve. The workspace shape is the
  smallest UX that makes a single `nros generate` call enough.
