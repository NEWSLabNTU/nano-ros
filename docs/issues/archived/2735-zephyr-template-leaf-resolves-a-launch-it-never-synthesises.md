---
id: 2735
title: "A copied-out Zephyr C/C++ template refuses to build: the workspace road looks for a SystemModel the leaf never has, before its census runs"
status: resolved
resolved_in: "the change that archives this issue"
type: bug
severity: high
area: [cli, sizing, templates, zephyr]
related: [issue-2520, issue-2482, issue-2137, issue-1556]
found: 2026-11-19
---

**Home:** sizing / templates. Found by nightly 44463027311, `check /
template-copy-out [zephyr]`. Exposed by issue 2520.

## What

All four Zephyr template copies fail at configure under
`NROS_PROBE_FAILURE=refuse`: `ament-workspace-port`,
`cpp-port-minimal-publisher`, `rclcpp-compat-smoke` and
`topic-state-monitor-port`, each its `zephyr/` leaf.

```
configure: zephyr: not configured — `zephyr`: entity counts not derived; configured
defaults kept, because what it declares could not be derived: cannot resolve the
SystemModel: cannot resolve `config/system_model.yaml`: its launch file
`<copy>/zephyr/build/nros/launch/zephyr/system.launch.xml` does not exist
```

These are the projects a new user starts from.

## Why

Each is a standalone C/C++ leaf: `system.toml` beside its own
`CMakeLists.txt` and `package.xml`, a `ROS2_MAIN` program, and no
`[[component]]` row. With no component row `nros sync` synthesises no launch
for it (`ws::synthesise_leaf_launch`), so it has no SystemModel by
construction. Its sizes come from its CENSUS: for a cross C/C++ leaf with a
program and nothing stated, `leaf_census::entity_source` is
`Census { explicit: false }` (issue 1556).

Each road treats it differently:

| road | leaf | what asked for a model |
| --- | --- | --- |
| cmake leaf (`plan_single_package_cmake`) | the `mps2-an385-freertos/` siblings | nothing: it takes `take --leaf` and is sized from its census (measured: 1 publisher, 1 timer) |
| cargo leaf (`plan_single_package`) | Rust leaves | not this question |
| workspace road (a West-driven leaf falls through to it) | the four `zephyr/` leaves | stage 3.4's image-qualified census take (`take_for_build`) and stage 3.5's `resolve_image` |

On the workspace road, the leaf's own census and seed (`configure_zephyr_leaf`,
`take --leaf`) run in stage 4. Stage 3.5 runs first and fails to resolve the
model. Before issue 2520 that failure only warned, so the step went on and
nothing visibly broke. Issue 2520 made it a refusal under `refuse`, and the
refusal is correct for a failed derivation. This was not one: the model road
had nothing to read, and the counts come from another road.

## Fix

- `entity_census::leaf_sized_without_model` asks the question once: a
  standalone leaf (`root` itself) with no `[[component]]` row, no authored
  `launch/`, no `[image] launch`, and a census entity source.
- `resolve_image` asks it before the model road and answers
  `NotDerived::LeafRoad`, a third reason beside "nothing declared" and
  "failed". It is reported as a note at any policy, and the resolve line says
  the counts come from the leaf's census. Asked first, so a launch that an
  earlier `[[component]]` row left under `build/` is never read.
- Stage 3.4 skips the image-qualified take for such a leaf, since stage 4
  takes `take --leaf` itself.

A leaf with no program stays `NoProgram`, which refuses on its own.

## Sweep

Every tracked leaf with `system.toml` beside a `package.xml` and a build file,
no `[[component]]` row and no `launch/`: the four `zephyr/` leaves above,
their four `mps2-an385-freertos/` siblings (cmake leaf road, sized from their
census), and five Rust test fixtures (cargo leaf road). Only the four
`zephyr/` leaves reached the model road.
