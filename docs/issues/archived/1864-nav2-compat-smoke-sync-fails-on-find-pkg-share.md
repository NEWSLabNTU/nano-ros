---
id: 1864
title: "`nav2_compat_smoke`'s `nros sync` fails resolving its `<include>` of an in-tree package — reported as `$(find secondary_node)`, a substitution the fixture does not use"
status: resolved
resolved: 2026-10-13
resolved_in: "683adbd29e (issue 1861); fixture comments corrected here"
type: bug
severity: medium
area: [cli, testing]
related: [1861, 1305, 0392, phase-212]
found: 2026-10-13
---

## Reported

Issue 1305 phase F measured `compile-check-fixtures.sh` over the 16 fixture
rows and found one red, before and after its own change: `nav2_compat_smoke`,
"whose `nros sync` fails on `$(find secondary_node)`". The consuming test,
`nav2_compat::n11_launch_xml_ros2_compat_smoke`, then fails with
`BuildFailed(... nav2_compat_smoke ... failed to build)`.

## Which layer refused it, and why

Not the substitution named in the report. `$(find <pkg>)` is the ROS 1
substitution; ROS 2 launch XML has `$(find-pkg-share <pkg>)` (and
`find-pkg-prefix` / `find-exec`), and the fixture has used that since
phase-212 M-F.20. `$(find …)` survived only in the fixture's COMMENTS
(`secondary.launch.xml`, three manifests, the test's doc), which is where the
report's wording came from. The only parser in nano-ros that still accepts
`$(find …)` is the legacy `nros-launch-parser` v1, which no build path spawns.

The real refusal, reproduced by reverting the fix below:

```
Error: sync: nros-launch-resolve failed for `demo_entry` (…/system.launch.xml):
Error: parsing …/system.launch.xml: Invalid substitution syntax:
Package 'secondary_node' not found. Ensure the package is installed and sourced.
```

It comes from the play_launch layer-2 resolver, statically linked into
`nros-launch-resolve`. play_launch `aab9507a` (inside the `d23720c7` pin) made
it find packages the way `ament_index` does: a prefix counts only if it
REGISTERS the package with `share/ament_index/resource_index/packages/<pkg>`.
That is correct ROS 2 behaviour, so the fix does not belong in play_launch. The
defect was ours. `nros sync`'s `synth_ament_prefix` gives the resolver a
throwaway prefix over the workspace's own packages, and it wrote only the
`share/<pkg>` symlinks, so the prefix registered nothing.

This is issue 1861, fixed in `683adbd29e` four minutes before 1305 phase F
landed, on a base that did not have it. That is why 1305 saw it both before
and after its change.

## Class

`synth_ament_prefix` (`cmd/ws.rs`) is the only place that synthesises an ament
prefix for the resolver. `nros sync` is the only production spawner of
`nros-launch-resolve` (`ws.rs`; the `nros-launch-parser` re-export is used by
tests only). So the fix covers every road. Its unit test
`synth_ament_prefix_tests::every_linked_package_is_registered_in_the_resource_index`
pins the marker. The misleading comments are corrected to the substitution
the fixture uses, and `$(arg …)` becomes `$(var …)`, the ROS 2 spelling, so the
next report names the right thing.

## Verified (2026-10-13, worktree on `7d41ce9a89`)

- `NROS_FIXTURE_ID=nav2_compat_smoke bash scripts/build/compile-check-fixtures.sh`
  rc=0, and the emitted `run_plan.rs` is real codegen, not the Placeholder stub.
- `cargo nextest run -p nros-tests --test nav2_compat` gives 1/1 PASS.
- **Mutation:** reverting 1861's marker write (`std::fs::write(index.join(..))`
  in `synth_ament_prefix`) and then running `just setup-cli` and rebuilding the
  fixture gives fixture rc=1 with the error above verbatim, and the test FAILS
  (`BuildFailed`). With the marker restored, the same three steps give rc=0
  and PASS.
- The full `compile-check-fixtures.sh` run is rc=2 only because of the PX4 rows
  (the PX4-Autopilot submodule is absent in this worktree). `nav2_compat_smoke`
  builds.
