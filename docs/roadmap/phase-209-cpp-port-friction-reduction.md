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

### 209.A.follow-up — capturing-lambda / `std::function` subscription callbacks
- [x] **Shipped (2026-05-30, branch `phase-209-cpp-port-friction-reduction`).**
      `rclcpp::Node::create_subscription` now accepts any callable (capturing
      lambda, `std::function`, member-fn bind, plain fn ptr). Implementation:
      polling-pump model — the compat opens nros's *polling* subscription
      (no SFINAE), heap-stores the user callable as `std::function`, and
      registers a pump callback the node's spin path invokes per sweep.
      `rclcpp::spin_some` / `spin` call `Node::pump()` before
      `nros::Executor::spin_once`. Lifetime: the captured `std::function`
      shares-ptr the subscription; cleanup is automatic when the subscription
      drops out of scope. Verified by reverting the 209.G synthetic
      `topic_state_monitor` to the natural `[state](const M&) { … }`
      capturing-lambda shape — compiles + links unchanged. (Native callback-
      arena path through the FFI user_data slot is a future optimization
      when per-spin polling overhead matters; for source-compat MVP this is
      the right trade.)

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
- [x] **Shipped (2026-05-30, branch `phase-209-cpp-port-friction-reduction`).**
      `cmake/compat/NrosRclcppCompat.cmake` defines the `ament_auto_*` /
      `ament_target_dependencies` / `ament_export_*` / `ament_auto_package` /
      `rclcpp_components_register_node` functions; the last synthesises a thin
      `int main()` per registration (single-binary embedded — no runtime
      composition). Force-includes `nros/rclcpp_compat.hpp` +
      `nros/rclcpp_components_compat.hpp` on every compat-built target so
      unmodified `#include <rclcpp/rclcpp.hpp>` source compiles without an
      include edit. Find-stubs under `cmake/compat/stubs/` cover ~24 of the
      most-cited ROS 2 packages (`ament_cmake_auto`, `ament_cmake`,
      `rcl`/`rmw`/`rosidl`, common msg packages). `Findrclcpp.cmake` +
      `Findrclcpp_components.cmake` create `rclcpp::rclcpp` /
      `rclcpp_components::component` IMPORTED INTERFACE targets aliasing to
      `NanoRos::NanoRosCpp` so the typical
      `target_link_libraries(... rclcpp::rclcpp)` resolves.
- [ ] **Acceptance:** an unmodified ROS 2 `CMakeLists.txt` builds against
      nano-ros after `include(NrosRclcppCompat)` is prepended. (E2e proof is
      209.G; this commit lands the surface 209.G will exercise.)

### 209.C — `RCLCPP_COMPONENTS_REGISTER_NODE` no-op shim
- [x] **Shipped (2026-05-30).** `packages/core/nros-cpp/include/nros/
      rclcpp_components_compat.hpp` defines the macro as a no-op. The
      209.B force-include applies it to every compat-built target; source
      lines using the macro just compile away. The cmake side
      (`rclcpp_components_register_node()` in NrosRclcppCompat.cmake) is
      what actually wires the entry point.

### 209.D — `nros-diagnostic-updater` C++ shim crate
- [x] **Shipped (2026-05-30, branch `phase-209-cpp-port-friction-reduction`).**
      `packages/core/nros-diagnostic-updater/` (header-only INTERFACE cmake
      target) exposes the upstream `diagnostic_updater::Updater` +
      `DiagnosticStatusWrapper` surface — constructors `(rclcpp::Node::SharedPtr,
      double period)` and the legacy `(node, period, freq_hz)` form; `add(name,
      cb)` + member-fn overload; `setHardwareID/getHardwareID` (+ snake-case
      alias); `force_update()`; `update()` (rate-limited self-publish); `broadcast
      (level, message)`; `setPeriod`. `DiagnosticStatusWrapper` mirrors `summary
      / summaryf / mergeSummary / clearSummary / add(key, value)` (string/int/
      double/bool overloads + `addf`). Out of scope deferred to a follow-up:
      `DiagnosticTask` class, `CompositeDiagnosticTask`, `FrequencyStatus` /
      `TimeStampStatus` from `update_functions.hpp`. The user runs `nano_ros_
      generate_interfaces(diagnostic_msgs ... LANGUAGE CPP)` so the generated
      message headers are available. cmake target alias
      `diagnostic_updater::diagnostic_updater` plus the nano-ros umbrella
      `NanoRos::DiagnosticUpdater`. `Finddiagnostic_updater.cmake` (under
      `cmake/compat/stubs/`) auto-`add_subdirectory`s this package, so a ported
      `find_package(diagnostic_updater)` + `target_link_libraries(...
      diagnostic_updater::diagnostic_updater)` resolves with no other changes.
- [ ] **Acceptance:** an Autoware node using `diagnostic_updater::Updater` +
      `setHardwareID` + a single `add(...)` task compiles + publishes
      `DiagnosticArray` on the configured topic in a nano-ros build. (E2e
      proof is 209.G; this commit lands the surface 209.G will exercise.)

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
- [x] **First iteration (2026-05-30, branch `phase-209-cpp-port-friction-
      reduction`).** Synthetic in-tree `topic_state_monitor` port at
      `examples/templates/topic-state-monitor-port/` — same shape as the
      upstream Autoware node (multi-topic liveness watchdog + per-topic
      `DiagnosticArray` publish). Builds + links against nano-ros through
      A+B+C+D with only the three compat lines in the CMakeLists. Surfaced
      one real gap (capturing-lambda subscription callbacks — filed as
      **209.A.follow-up** above). The synthetic node uses stateless
      fn-pointer subscriptions + global state as the today-workaround,
      documented inline. Native build verified (`build/topic_state_monitor`).
- [x] **Iter 2 — vendor the canonical upstream source + book page (2026-05-30,
      branch `phase-209-cpp-port-friction-reduction`).** The 209 scope is
      ROS-2-generic, not Autoware-specific; the right "real" target is the
      canonical ROS 2 tutorial source (any small upstream node would do —
      Autoware was just one of the survey's *measurement* fixtures, not the
      acceptance target). Vendored `examples/templates/cpp-port-minimal-
      publisher/` — the upstream ROS 2 tutorial's `minimal_publisher.cpp`
      **verbatim**, builds against nano-ros through the 209.A–D compat
      surface with the three-line CMakeLists glue (NANO_ROS_PLATFORM +
      add_subdirectory, NrosRclcppCompat.cmake include, the per-pkg
      nros_generate_interfaces). One compat-surface gap surfaced + closed
      while porting: `rclcpp::TimerBase` + `Node::create_wall_timer(period,
      callback)` weren't in 209.A — added (pump-dispatched wall-timer with
      capturing-lambda callback support, mirrors the subscription pump).
      Two remaining caveats are codegen-side (FixedString vs std::string;
      umbrella header path vs `<pkg>/msg/<name>.hpp`) — tracked under
      Phase 209.E.
- [x] **Book page** `book/src/getting-started/porting-a-cpp-node.md` — landed.
      Walks the three-line glue + a "what works / what's codegen-cosmetic /
      what's deferred" table + cross-refs the in-tree fixture.
- [ ] **Acceptance:** the upstream example also boots on `native_sim`
      (Zephyr). (Native posix is verified; Zephyr boot is a per-platform
      cmake configure of the same source — separate confirmation.)

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
