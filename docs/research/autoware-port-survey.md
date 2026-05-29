# Autoware → nano-ros porting effort survey

**Question.** How much work does it take to port a normal ROS 2 C++ node into
nano-ros? Ideally — "just change the build scripts + a few `#include`s." Are
there Autoware nodes that fit that shape, especially safety-island candidates?

**TL;DR.** A handful of small, control-only nodes look genuinely close to a
"swap rclcpp → nros-cpp + add codegen + go" port:

- `autoware_external_cmd_selector` (316 SLOC) — pure cmd-source state machine.
- `topic_state_monitor` (465 SLOC) — topic-liveness watchdog.
- `autoware_steer_offset_estimator` (212 SLOC) — light steering-offset filter.

Everything heavier either pulls in `Boost.Geometry` / `Eigen` / `PCL` /
planning + perception msgs (collision_detector, AEB, …) or wants
`autoware_universe_utils` deeply (polling_subscriber, timer helpers, geometry)
which nano-ros doesn't have yet. The Sentinel project (`NEWSLabNTU/autoware_sentinel`)
already chose the *full Rust rewrite* path for its 7 safety nodes (971 `.rs`
files, 20 `.cpp`), so the "minor change" route is largely untested in our
codebase. This survey scopes what it would take to validate it.

## What "minor changes" requires from nano-ros

Three things have to exist for a C++ Autoware node to compile mostly-unchanged
against nano-ros:

1. **`nros-cpp` covers the rclcpp surface the node uses.** Today nros-cpp
   mirrors rclcpp 0.7.0 (`Node::create_publisher/subscription/service/client`,
   `Publisher<M>`, parameters, executor spin) — enough for the simple nodes.
   Gaps: `rclcpp_components::ComponentManager`, `rclcpp_lifecycle::LifecycleNode`
   for managed nodes, `rclcpp::Timer` walls (we have `create_wall_timer`).
2. **The message packages the node `#include`s are code-generated.** Today
   only the bundled base interfaces (`std_msgs`, `builtin_interfaces`, …) ship
   inside the `nros` CLI. Autoware nodes need `autoware_control_msgs`,
   `autoware_vehicle_msgs`, `autoware_system_msgs`, `autoware_adapi_v1_msgs`,
   `tier4_control_msgs`, `tier4_external_api_msgs`, `tier4_auto_msgs_converter`,
   `nav_msgs`, `geometry_msgs`, … — runnable via `nros generate cpp` against the
   `.msg` sources, but those `.msg` files have to be present (the autoware
   workspace or an extracted SDK).
3. **Three Autoware *helper* libraries are pervasive and aren't ROS-runtime
   abstractions — they're auxiliary code that almost every node touches:**
   - `diagnostic_updater` (publishes `diagnostic_msgs/DiagnosticArray`)
   - `autoware_universe_utils` (polling subscriber, ROS-timer wrappers, tiny
     geometry helpers, debug publisher)
   - `autoware_vehicle_info_utils` (loads vehicle wheelbase / track / inertia
     from a ROS parameter set or yaml).
   For a true drop-in port, each needs a nano-ros-compatible build. The
   Sentinel project already carries Rust ports of `autoware_universe_utils`
   and `autoware_vehicle_info_utils` (`~/repos/autoware_sentinel/src/`), but
   the *C++ headers* an unchanged Autoware source compiles against don't
   exist as nano-ros crates yet.

So "minor changes" really means: **nros-cpp + the right `nros generate cpp`
codegen + a small compat shim for `diagnostic_updater`** (the universe_utils
and vehicle_info_utils surfaces can be replaced with stubs or skipped for
diagnostic-light nodes). That shim is the single biggest blocker between
"swap build scripts" and the current shape; the simplest nodes don't use the
shim much, so they're the natural first proof.

## Candidate ranking

Audit of `~/repos/autoware_universe/{control,vehicle,system}` packages.
`SLOC` = total C++ lines (`.cpp` + `.hpp`). `Tier` = expected porting effort.

| Package | SLOC | Tier | Why it fits / what blocks it |
|---|---|---|---|
| **`autoware_external_cmd_selector`** (control/) | 316 | **1 (easy)** | Pure cmd-source state machine; identical pattern to the already-ported `vehicle_cmd_gate`. Deps: `autoware_vehicle_msgs`, `tier4_control/external_api_msgs`, `tier4_auto_msgs_converter` (header-only), `diagnostic_updater`. No filesystem, no geometry, no autoware_universe_utils. |
| **`autoware_steer_offset_estimator`** (vehicle/) | 212 | **1 (easy)** | A few-line LPF on (steer_cmd − steer_measured). Deps: `autoware_vehicle_msgs`, `geometry_msgs`, `tier4_debug_msgs`, `diagnostic_updater`, `autoware_universe_utils` (used lightly — timer + debug publisher). Replace the universe_utils calls with raw rclcpp ones. |
| **`topic_state_monitor`** (system/) | 465 | **1 (easy)** | A liveness watchdog (frequency + age check per topic) — same shape as Sentinel's `heartbeat_watchdog`. Deps: `diagnostic_updater`, `tf2_msgs`. Generic-typed subscriptions but the type erasure is small. |
| **`hazard_status_converter`** (system/) | 184 | 2 (moderate) | Converts diag-graph status → `autoware_system_msgs/HazardStatus`. Small, but pulls `autoware/universe_utils/ros/polling_subscriber` + `diagnostic_graph_utils` (a sibling Autoware package, also has to be ported). |
| **`autoware_external_cmd_converter`** (vehicle/) | 365 | 2 (moderate) | External cmd → vehicle cmd. **Reads accel/brake LUTs from CSV files at startup** — needs to be replaced with compile-time-baked tables for an embedded port. |
| **`autoware_joy_controller`** (control/) | 1 064 | 2 (moderate) | Joystick → ackermann/external cmd mapping. Larger but mostly `switch`-on-enum + msg construction. No heavy deps. |
| `autoware_collision_detector` (control/) | 653 | 3 (skip) | `Boost.Geometry`, `autoware_perception_msgs`, `autoware_planning_msgs`, `autoware_vehicle_info_utils`. Perception-heavy. |
| `autoware_autonomous_emergency_braking` (control/) | 2 104 | 3 (skip) | Perception + planning msgs + likely PCL. Big. |
| `autoware_obstacle_collision_checker` (control/) | 841 | 3 (skip) | Planning msgs + geometry. |
| `predicted_path_checker` (control/) | 2 092 | 3 (skip) | Planning trajectories. |
| `autoware_raw_vehicle_cmd_converter` (vehicle/) | 1 273 | 3 (skip) | 7 source files + CSV map loading. |
| `system_monitor` (system/) | — | 3 (skip) | Reads `/proc` — Linux-only, not a safety-island fit. |
| `duplicated_node_checker` (system/) | 142 | 3 (skip) | Calls `Node::get_node_names()` — discovery-API surface nano-ros doesn't expose. |
| `dummy_diag_publisher` (system/) | 245 | — | Test utility, not safety logic. Skip on relevance. |

## The Tier-1 trio — what a real port looks like

For each, the steps to compile against nano-ros are:

### `autoware_external_cmd_selector` (control)
- **Codegen.** `nros generate cpp` for `autoware_vehicle_msgs`, `tier4_control_msgs`,
  `tier4_external_api_msgs`, `std_msgs`, `tier4_auto_msgs_converter` (the last is
  a header-only converter library, not a msg pkg — vendor its header).
- **Build glue.** Replace the package's `CMakeLists.txt` `ament_*` calls with
  `set(NANO_ROS_PLATFORM <plat>) + add_subdirectory(<nano-ros>) +
  target_link_libraries(<app> PRIVATE NanoRos::NanoRos)` (the standard
  in-tree consumption shape from CLAUDE.md).
- **Source.** `#include` paths shift `rclcpp/rclcpp.hpp` → `nros/nros.hpp`;
  `rclcpp::Node` → `nros::Node`. The body (a state machine over an
  `ExternalCommandSelectorMode` enum + per-source TTL) needs no logic change.
- **Stub.** Either implement a 2-method `diagnostic_updater::Updater` against
  `nros-cpp` (publishes `diagnostic_msgs/DiagnosticArray` at a fixed rate) or
  `#ifdef`-out the diagnostic block — the rest of the node doesn't read it.

Estimated effort: **0.5 – 1 day** once `nros generate cpp` against autoware
msgs is wired, dominated by `tier4_auto_msgs_converter` vendoring + the
diagnostic stub.

### `topic_state_monitor` (system)
Same shape — codegen the msgs the monitored topics carry + provide the
`diagnostic_updater` stub. The "monitor any topic" generic-typed subscription
maps onto `nros-cpp`'s typed `create_subscription<M>`; the node templates over
the message type per instance, no new nano-ros mechanism required.

Estimated effort: **0.5 day**.

### `autoware_steer_offset_estimator` (vehicle)
Adds the `autoware_universe_utils` surface (`Timer` + `DebugPublisher`).
Replace with `node.create_wall_timer` + a plain typed publisher;
`tier4_debug_msgs` codegen + the `diagnostic_updater` stub.

Estimated effort: **0.5 day**.

## What the trio buys us

- A measured floor on the "minor-change port" cost — concrete answer to the
  user's question instead of a hand-wave.
- The `diagnostic_updater` stub becomes reusable for every later port.
- A real test of `nros generate cpp` against autoware-msgs — which exposes
  whether the codegen handles the (numerous) cross-package `# include` chains
  cleanly.
- If all three drop in, the path is real and the heavier nodes (joy_controller,
  external_cmd_converter) become a question of effort, not of feasibility.
- If the diagnostic stub balloons or `nros generate cpp` chokes on autoware
  msgs, that's the honest "you'd be better off rewriting in Rust" answer the
  Sentinel project chose.

## Out of scope for this survey

- **A perception or planning node** — every one of those drags in `PCL`,
  `Boost.Geometry`, `Eigen`, or the autoware planning msg stack. Not a
  safety-island fit and not a fair "minor change" test.
- **A managed lifecycle node** — `rclcpp_lifecycle::LifecycleNode` isn't in
  nros-cpp today; lifecycle-managed Autoware nodes can't be drop-in ported
  before that lands.
- **Yaml-loaded parameters** — Autoware loads vehicle constants from a
  yaml/launch tree; embedded nano-ros doesn't have a yaml loader. For the
  Tier-1 trio the loaded params are few (4–8 floats) and can be baked into a
  C++ header until a runtime path lands.

## Suggested next step

Pick **`topic_state_monitor`** (smallest + most generic) as the first port
attempt: it stresses `nros generate cpp` over a couple of common msg packages
+ exercises the diagnostic shim without dragging vehicle-msg specifics. If it
goes clean in ~half a day, do `external_cmd_selector` as the second proof
(adds the converter-header vendoring + cmd-source state-machine shape). The
two together cover the "swap build scripts + minor source changes" promise
end-to-end, or surface the first real blocker.
