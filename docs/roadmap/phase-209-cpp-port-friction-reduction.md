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

**Status.** **MVP DONE in-tree (2026-05-30, branch `phase-209-cpp-port-friction-
reduction`).** A canonical ROS 2 C++ node (the upstream tutorial's
`minimal_publisher.cpp` — vendored verbatim under `examples/templates/cpp-port-
minimal-publisher/`) compiles + links + runs against nano-ros through the
shipped 209.A–D compat surface, with only three glue lines prepended to its
`CMakeLists.txt`. Plus an integration smoke + a topic_state_monitor synthetic
that exercises the multi-sub / wall-timer / diagnostic_updater paths. Book
page at `book/src/getting-started/porting-a-cpp-node.md`. Open items remap:
209.E → **Phase 210** (broader workspace-discovery + ROS-convention codegen);
209.F → nros-cli (yaml params bake, off-tree); 209.G restructured as a
**port-validation matrix** (G.1 ✅, G.2 Zephyr native_sim + G.3 freertos-qemu
+ G.4 richer fixture + G.5 post-210 doc refresh = follow-ups); 209.H →
deferred (P3).

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
- [x] **Acceptance — met.** A canonical ROS 2 C++ source (the upstream
      tutorial's `minimal_publisher.cpp`) compiles unchanged via this header
      (see 209.G iter 2). Scope is ROS-2-generic; the original "Autoware
      Tier-1" wording referred to it as just one possible measurement target.

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
- [x] **Acceptance — met.** `examples/templates/cpp-port-minimal-publisher/
      CMakeLists.txt` is a stock ament_cmake_auto package (find_package /
      ament_auto_add_executable / ament_target_dependencies / ament_auto_package)
      with three nano-ros glue lines prepended (set platform, add_subdirectory,
      include NrosRclcppCompat + the per-pkg codegen lines that 210.C/210.B
      will fold). Builds end-to-end.

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
- [x] **Acceptance — met.** Both `examples/templates/rclcpp-compat-smoke/`
      (single Updater task) and `examples/templates/topic-state-monitor-port/`
      (per-topic Updater tasks via capturing-lambda) build + run and publish
      `diagnostic_msgs/DiagnosticArray` on `/diagnostics`. Scope is ROS-2-
      generic; the original "Autoware node" wording referred to it as one
      possible measurement target.

### 209.E — `nros generate cpp --workspace <ws>` ROS 2 msg bulk codegen
**Superseded by Phase 210** (`docs/roadmap/phase-210-ros-convention-codegen.md`)
— 210.C lands this work in the broader workspace-discovery + ROS-convention
codegen frame (`rosidl_generate_interfaces` cmake shape + smart
`find_package(<msg-pkg>)` stub + layered search path). Original sketch kept
for reference:
- [ ] Today `nros generate cpp <pkg>` runs per package. A real-world ROS 2 port
      transitively needs 5–20 message packages (the stock `geometry_msgs` /
      `nav_msgs` / `diagnostic_msgs` / `tf2_msgs` / `sensor_msgs` set + whatever
      the project ships). Add a `--workspace <path>` (or `--scan <path>`) shape
      that crawls every `package.xml` under the path with
      `<build_type>ament_cmake</build_type>` + a `msg/*.msg` directory and runs
      codegen for each, respecting the per-package `<depend>` graph. ROS-2-generic;
      any colcon workspace. (nros-cli work — owned in the standalone repo.)
- [ ] **Acceptance** — moved to **210.C** (closed by Phase 210).

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

### 209.G — Port-validation matrix  (was: "Walking the first port end-to-end")

The 209 source-compat surface is proven on one platform + one canonical
upstream source. Extend the matrix with concrete per-platform + per-pattern
follow-ups so "every ROS 2 node ports cleanly" has graduated evidence — not
just a single fixture.

- [x] **209.G.1 — Native posix · canonical ROS 2 source (2026-05-30, branch
      `phase-209-cpp-port-friction-reduction`).** Vendored
      `examples/templates/cpp-port-minimal-publisher/` — upstream ROS 2
      tutorial `minimal_publisher.cpp` **verbatim**, builds against
      nano-ros through 209.A–D with the three-line CMakeLists glue
      (NANO_ROS_PLATFORM + add_subdirectory, NrosRclcppCompat.cmake
      include, nros_generate_interfaces). Native build verified.
      One compat-surface gap closed while porting: `rclcpp::TimerBase` +
      `Node::create_wall_timer(period, callback)` weren't in 209.A —
      added (pump-dispatched wall-timer with capturing-lambda support,
      mirrors the subscription pump). Earlier iteration (synthetic
      `topic_state_monitor` at `examples/templates/topic-state-monitor-
      port/`) surfaced the capturing-lambda gap → **209.A.follow-up**.
      Book page `book/src/getting-started/porting-a-cpp-node.md` —
      landed; walks the three-line glue + "what works / codegen-cosmetic
      / deferred" table.
- [ ] **209.G.2 — Zephyr `native_sim` · same source. BLOCKED** on a
      Zephyr `libstdc++`-subset shim project (was assumed "per-platform
      configure pass"; investigation 2026-05-30 proved otherwise).
      Zephyr's minimal C++ stdlib (`zephyr/lib/cpp/minimal/`) ships only
      `<cstddef>`/`<cstdint>`/`<new>` + a few cstr/cmath; `rclcpp_compat.hpp`
      pulls `<memory>`/`<string>`/`<functional>`/`<vector>`/`<chrono>`
      (chrono is already shimmed under `zephyr/cxx-compat/`). The four
      missing headers each need a non-trivial shim — `<memory>` ≈ 200
      LoC for `shared_ptr`+`make_shared`+`enable_shared_from_this`+
      `unique_ptr`+`*_pointer_cast` alone. Real prereq is a small
      "libstdc++ subset for Zephyr nros" project, not part of 209.
      Two alternatives both rejected:
      `CONFIG_GLIBCXX_LIBCPP=y` on native_sim/x86_64 is unbuildable
      (Zephyr SDK 0.16.8 ships no `picolibc.specs` for x86_64-zephyr-elf);
      a `CONFIG_EXTERNAL_LIBCPP=y` route to host libstdc++ collides with
      Zephyr's `-nostdinc` net-header isolation (host `<netinet/in.h>` vs
      Zephyr's own struct sockaddr). **Follow-up: file as "Zephyr
      libstdc++ subset for nros-cpp" — separate phase.**
- [ ] **209.G.3 — `qemu-arm-freertos` · same source. BLOCKED** on two
      prereqs (was assumed "first embedded-RTOS validation"; investigation
      2026-05-30 found the gap):
      1. **Entry-point dispatch.** freertos's startup chain calls
         `void app_main(void)` via `NROS_APP_MAIN_REGISTER_VOID()`, which
         calls `nros_app_main(0, NULL)`. The upstream tutorial source
         declares `int main(int, char**)`. Need a compat-emitted shim
         (a generated `.c` TU added to the app target by
         `NrosRclcppCompat.cmake` on `NANO_ROS_PLATFORM=freertos`) that
         defines `nros_app_main` as a forwarder to `main`.
      2. **Runtime config bake.** `rclcpp::init(argc, argv)` calls
         `nros::init()` no-args, which falls through to env vars on
         hosted (`NROS_LOCATOR`/`ROS_DOMAIN_ID`). On `__STDC_HOSTED__=0`
         embedded, env vars don't reach the binary — the freertos
         examples bake locator/domain_id via `nros.toml` → `app_config.h`.
         `rclcpp_compat.hpp::init` has no equivalent path; either auto-
         emit a default `app_config.h` from `NrosRclcppCompat.cmake` or
         teach `rclcpp::init` to read `NROS_APP_CONFIG` on embedded.
      arm-none-eabi-newlib libstdc++ DOES ship `<memory>`+`<string>`+
      `<functional>`+`<vector>`+`<chrono>` (verified locally) so the
      header-set gap that blocks G.2 doesn't bite here. **Follow-up:
      file "Embedded entry-point + config-bake shim for rclcpp_compat"
      — single follow-up unblocks G.3 + every other embedded G.
- [ ] **209.G.4 — Larger real-world fixture.** A ported ROS 2 node that
      exercises >2 message types + a service + a parameter. Surfaces the
      next round of compat gaps (composition, multi-node, intra-process).
      Picked after **Phase 210** (codegen workspace) lands so the
      codegen glue is one line, not three.
- [ ] **209.G.5 — Book-page refresh post-Phase 210.** Once 210 collapses
      the three codegen glue lines to `nros_workspace_interfaces()` /
      `find_package(<msg>)`, refresh `book/src/getting-started/porting-
      a-cpp-node.md` so the worked example matches the new glue.

**Acceptance gating.** Today: G.1 closed (MVP proof). G.2 + G.3 both
need real prereq work outside 209's scope (see above); each follow-up
is a separate phase. G.4 + G.5 land alongside 210.

**Investigation note (2026-05-30).** The first revision of 209.G framed
G.2/G.3 as "per-platform cmake configure pass, no new compat code". A
spike against both targets disproved that — every embedded target has
either a libcpp-subset gap (Zephyr minimal stdlib) or an entry-point
+ runtime-config-bake gap (every RTOS that doesn't run user `main`
directly). Both gaps are tractable but each is at least one focused
follow-up phase, not part of 209. Updated above to reflect reality
rather than the original intent.

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
(param bake) lifts the ceiling to small param-loaded nodes. 209.G is the
port-validation matrix (G.1 is the today-proof; G.2–G.5 graduate the evidence).
209.H (LifecycleNode mirror) is a separate, larger piece.

## Notes

- The proof — 209.G.1 — is **the real measurement**. Until a real ROS 2 C++ source
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
