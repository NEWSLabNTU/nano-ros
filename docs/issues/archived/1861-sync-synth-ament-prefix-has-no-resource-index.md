---
id: 1861
title: "`nros sync`'s synthesised ament prefix registers no package, so `$(find-pkg-share <in-tree pkg>)` fails since the play_launch `d23720c7` pin"
status: resolved
type: bug
severity: medium
area: [cli, build]
related: [0392, phase-486]
found: 2026-10-13
---

## Measured

The compile-check fixture `nav2_compat_smoke` fails its `nros sync` in every
tier-2 build since the `play_launch` pin moved to `d23720c7` (phase-486 W7's
"before" side and its "after" side alike):

```
Error: parsing …/nav2_compat_smoke/src/demo_entry/launch/system.launch.xml:
Invalid substitution syntax: Package 'secondary_node' not found. Ensure the
package is installed and sourced.
```

`secondary_node` is a package in the fixture's own `src/`, included with
`$(find-pkg-share secondary_node)`.

## Cause

`cmd/ws.rs::synth_ament_prefix` (issue 0392 C) gives the resolver a throwaway
ament prefix over the workspace's own packages: `<tmp>/share/<pkg>` symlinks,
prepended to `AMENT_PREFIX_PATH`.

play_launch `aab9507a` ("fix(parser): find packages the way ament_index does",
2026-10-11, inside the `d23720c7` pin) changed how the resolver finds a
package. A prefix now counts only if it REGISTERS the package with
`share/ament_index/resource_index/packages/<pkg>`, which is `ament_index`'s
rule and `ros2 launch`'s. The synthesised prefix had the share directory and
no marker, so it registered nothing.

## Fix

`synth_ament_prefix` writes the empty marker for every package it links, as a
real install carries. Unit test:
`synth_ament_prefix_tests::every_linked_package_is_registered_in_the_resource_index`.

## Verified

See the PR: `nav2_compat_smoke` builds in a tier-2 compile-check run.
