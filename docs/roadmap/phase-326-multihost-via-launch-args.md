# phase-326 — multi-host via launch arguments

**Resolves:** [issue 0363](../issues/0363-machine-attr-is-ros-1-not-ros-2.md)
**Status:** Not started

## Why

`<node machine="robot1">` is ROS 1 roslaunch syntax. ROS 2 has no
multi-machine launch and its XML frontend rejects the attribute outright, so
the four `multihost.launch.xml` fixtures cannot be run by `ros2 launch`.
Upstream removed `machine=` and `model::Deploy.host` on 2026-07-31; see
issue 0363 for the evidence.

## What replaces it

A standard `<arg>` plus `if=` conditions. The host is selected when the
launch file is *resolved*, so the resulting SystemModel already contains
exactly that host's nodes:

```xml
<launch>
  <arg name="host" default="all"/>
  <node pkg="talker_pkg" exec="talker" name="talker"
        if="$(eval '&quot;$(var host)&quot; in (&quot;robot1&quot;, &quot;all&quot;)')"/>
  <node pkg="listener_pkg" exec="listener" name="listener"
        if="$(eval '&quot;$(var host)&quot; in (&quot;robot2&quot;, &quot;all&quot;)')"/>
</launch>
```

A node with no `if=` is shared across hosts, matching the old "no `machine=`
means every host" rule.

The partition moves from bake time to resolve time, so `Plan::for_host` and
`PlanNode.host` are deleted rather than reimplemented.

## Work

1. **Fixtures.** Rewrite `multihost.launch.xml` in all four example
   workspaces (`examples/workspaces/{c,cpp,mixed,rust}/src/demo_bringup/launch/`)
   to the arg + condition form. Verify each one loads under stock
   `ros2 launch`, which none of them do today.

2. **Plan.** Delete `PlanNode.host` and `Plan::for_host` from
   `packages/cli/nros-cli-core/src/codegen/entry/mod.rs` (`for_host` at
   line 118, `host` field at line 201), plus the
   `plan_for_host_partitions_by_machine` unit test at line 693.

3. **CLI and macro.** `nros codegen entry --host <id>`
   (`packages/cli/nros-cli-core/src/cmd/codegen.rs:131-134`) and
   `nros::main!(launch=…, host="…")`
   (`packages/core/nros-macros/src/main_macro.rs:81-86`) become generic
   launch-argument passing. `host:=<id>` is then an ordinary launch
   argument with no special handling, which also unblocks any other
   conditional a workspace wants to drive from the bake.

   Decide during implementation whether to keep `--host` as sugar for
   `--launch-arg host:=<id>` or to remove it. Upstream's position is that no
   argument name should get special treatment; sugar that expands to a
   launch argument does not violate that, a code path keyed on the name
   does.

4. **system.toml.** `[deploy.robot1]` / `[deploy.robot2]` blocks in the
   example workspaces stop doubling as placement selectors. Each needs an
   explicit `nodes = [..]` list, or the blocks are removed if the per-host
   models make them redundant. See issue 0363 for why the `by_machine`
   fallback went away.

5. **Tests.** `packages/testing/nros-tests/tests/multihost_partition_bake.rs`
   asserts "the deploy-target id == the launch `machine=` id"
   (`multihost_deploy_targets_match_baked_hosts`, line 99) — that premise
   dissolves. Rewrite it to assert that resolving with `host:=robot1`
   produces a model containing only robot1's nodes.
   `multihost_e2e.rs` and the `Workload::Multihost` matrix entry
   (`packages/testing/nros-tests/src/matrix.rs:264`) need the new invocation
   form.

6. **Vendored crates.** Bump `packages/cli/third-party/ros-launch-manifest`
   and `packages/cli/third-party/ros-launch-resolve` (both copies) only
   after steps 2-5 land. Bumping earlier fails to compile.

## Net effect

One code path disappears. Multi-host becomes an ordinary launch-argument
conditional that stock ROS 2 tooling understands, and the launch files
become runnable by `ros2 launch` for the first time.
