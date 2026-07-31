# 0364 — `<node machine=>` is ROS 1 syntax, not ROS 2

**Status:** Open
**Filed:** 2026-07-31
**Affects:** `nros codegen entry --host`, `nros::main!(host=…)`,
`Plan::for_host`, the four `multihost.launch.xml` example workspaces
**Upstream:** play_launch / ros-launch-resolve / ros-launch-manifest

## Summary

The multi-host partitioning added in phase-211.F is built on
`<node machine="robot1">`, which is ROS 1 roslaunch syntax. ROS 2 has no
such attribute and no multi-machine launch at all.

## Evidence

- `launch_ros`'s `Node.parse()` reads `args`, `ros_args`, `name`,
  `exec_name`, `pkg`, `exec`, `namespace`, and the `remap`/`param` children.
  There is no `machine`. The string does not appear anywhere in
  `ros2/launch_ros`.
- ROS 2's XML frontend is strict:
  `launch_xml/entity.py::assert_entity_completely_parsed()` raises on any
  attribute no action consumed. Running
  `examples/workspaces/rust/src/demo_bringup/launch/multihost.launch.xml`
  through Humble's frontend gives:

  ```
  ValueError: Unexpected attribute(s) found in `node`: {'machine'}
  ```

  All four example workspaces (`c`, `cpp`, `mixed`, `rust`) fail the same
  way. They cannot be run by `ros2 launch`.
- The ROS 2 multi-machine launch proposal (`ros2/design` PR #255, opened
  2019-09-16) was **closed without merging**. Under peer-to-peer DDS
  discovery there is no central launcher to distribute processes from.
- play_launch's Rust parser accepted `machine=` while its Python parser
  (real `launch`) rejected it — a parser-parity break that went unnoticed
  because nothing exercised the Python path on these fixtures.

## Upstream change

Removed on 2026-07-31:

- `play_launch_parser`: the `machine` capture, end to end
- `ros-launch-resolve`: `launch_dump::NodeRecord.machine` and the
  `machine=` → `execution.deploy[fqn].host` mapping in `model_builder`
- `ros-launch-manifest`: **`model::Deploy.host` is gone**, along with the
  `by_machine` placement fallback added for issue 0291

nano-ros vendors `ros-launch-manifest` and `ros-launch-resolve` under
`packages/cli/third-party/` at pinned revisions, so **the build is not
broken today**. Bumping either vendored copy before migrating will fail to
compile: `PlanNode.host` derives from `Deploy.host`.

Two consequences to plan for:

1. `Deploy.host` no longer exists. Anything reading it needs a different
   source — or, per the migration, no source at all.
2. With multiple in-scope `[deploy.*]` blocks, a node can no longer be
   placed by its `machine=`-derived host. Every node needs an explicit
   `nodes = [..]` entry, or placement errors with "is not placed". This is
   the pre-0291 behavior, restored because the fact 0291's fallback read no
   longer exists.

## Resolution

phase-326 — multi-host via launch arguments. The partition moves from a
bake-time filter (`Plan::for_host`) to a resolve-time launch argument, which
deletes a code path rather than replacing it.
