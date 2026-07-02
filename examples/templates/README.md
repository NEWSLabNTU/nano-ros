# examples/templates/

Multi-platform copy-out templates — Pattern A workspace layouts
and similar scaffolds that don't belong to a single
`<plat>/<lang>/<rmw>/<example>` cell. Each subdirectory is a
standalone project you can copy into your own tree and customize.

For the promoted Node + Bringup + Entry workflow examples, use
`examples/workspaces/`.

## Contents

- `pure-c-workspace/` — Phase 223 pure-C 3-role workspace: C Node
  pkgs, C Bringup pkg, and a C Entry pkg generated through
  `nano_ros_entry(LANG c LAUNCH ...)`.

- `c-and-cpp-mixed-workspace/` — Phase 223 mixed-language 3-role
  workspace: C Node pkg, C++ Node pkg, Bringup pkg, and C++ Entry pkg.

- `multi-package-workspace/` — mixed C + C++ + Rust packages
  sharing one nano-ros install via `CMAKE_PREFIX_PATH` and
  Cargo `[patch.crates-io]`. Demonstrates the Phase 123 Pattern A
  layout where each downstream workspace pins one nano-ros source
  checkout under `src/nano-ros/`.

- `multi-node-workspace/` — the canonical Rust 3-role workspace
  (`docs/design/0024-multi-node-workspace-layout.md` §11): Node pkgs
  + a declarative Bringup pkg + an Entry pkg (`nros::main!(launch =
  …)`), copy-out-and-rename ready.

- `multi-node-workspace-cpp/` — the C++ counterpart (Phase 240.2b /
  RFC-0043 / phase-257): typed component Node pkgs routed through
  `nano_ros_entry(... TYPED)` to the real executor
  (`Board::run_components`).

- `cpp-port-minimal-publisher/` — Phase 209.G iter 2: the upstream
  ROS 2 "minimal publisher" tutorial C++ node vendored **unmodified**,
  building against nano-ros via the Phase 209.A–D rclcpp-compat
  surface with only three build-glue lines changed.

- `rclcpp-compat-smoke/` — integration test for the Phase 209
  MVP-quartet (`rclcpp_compat.hpp` + `NrosRclcppCompat.cmake` +
  `rclcpp_components_compat.hpp` + `nros-diagnostic-updater`): a
  stock `rclcpp::Node` subclass with `diagnostic_updater::Updater`
  built against nano-ros with one glue line per layer.

- `topic-state-monitor-port/` — Phase 209.G first iteration: a
  synthetic in-tree port modeled on Autoware's `topic_state_monitor`,
  exercising multi-subscription + per-topic diagnostic patterns a
  real port would hit (no vendored upstream source yet).

- `local-msg-package/` — Phase 210.A.4 fixture demonstrating
  ROS-convention codegen: verbatim ROS 2 msg packages (workspace +
  cross-workspace deps) consumed alongside AMENT-installed messages
  by a verbatim ROS 2 C++ consumer node.

- `workspace-shadowing/` — Phase 210.F.4 fixture proving the
  workspace-over-AMENT interface-package shadowing contract: a
  workspace `std_msgs` (carrying a message AMENT's does not ship)
  shadows the AMENT-installed `std_msgs`.

- `zephyr-byo/` — Zephyr "bring-your-own west workspace" starter
  (Phase 205.A): a `west.yml` pinning a tested Zephyr + the nano-ros
  module import, plus a zenoh `std_msgs/Int32` talker app skeleton.
  Source for the standalone `nano-ros-zephyr-example` repo; see its
  `README.md` for the current `west init → nros setup zephyr (patches
  applied automatically) → west build → run` quickstart (e2e-verified,
  Phase 202; the earlier `west patch` step was retired in Phase 208.E.9
  — patches now apply during `nros setup zephyr` provisioning).
