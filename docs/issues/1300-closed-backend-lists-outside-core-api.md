---
id: 1300
title: "Closed backend lists OUTSIDE core/api have no gate — `check-rmw-agnostic` reaches exactly RFC-0071's two bullets, and the CLI scaffold, colcon's `RMW_BACKENDS` and `board.knobs.zenoh` sit beyond them"
status: open
type: tech-debt
area: ci, cli, rmw
severity: low
found: 2026-09-11
related: [1219, 0934, RFC-0071, phase-444]
---

## What

Issue 1219 counted five closed backend lists; phase-439 W4 removed three and
phase-444 W4.b wrote `check-rmw-agnostic`, whose REACH is exactly RFC-0071 §
Verification: `packages/core/**`, `packages/api/nros{,-c,-cpp}/**` and
`cmake/NanoRosRmwDispatch.cmake`. That is the rule the RFC states, and the gate
enforces all of it. The lists 1219 found beyond it are a DIFFERENT rule — "no
tool enumerates the backends" — and are recorded here rather than dropped when
1219 closed:

| site | shape | `uorb`? |
| --- | --- | --- |
| `packages/cli/cargo-nano-ros/src/workspace_scaffold.rs` | `"cyclonedds" \| "zenoh" \| "xrce" => {}` else `bail!` | rejected |
| `packages/cli/colcon-cargo-ros2/colcon_nano_ros/task/nros/build.py` | `RMW_BACKENDS = ("zenoh", "xrce", "cyclonedds")` | absent |
| `packages/cli/nros-cli-core/src/cmd/config.rs` | `board.knobs.zenoh.tx`, `"zenoh.tx.batch"` (RFC-0071 D8) | n/a |
| `packages/cli/nros-cli-core/src/orchestration/bridge_gen.rs` | the first spelling of `rmw_crate_ident`'s table | absent |

## Why this is not simply "widen the gate"

1219's first attempt measured it: over the build-decision surface, comments
stripped and names matched only as quoted literals or alternations, **39 files**,
roughly 40 % of them test-plan coordinates, bridge demos that name two backends
because bridging them is the point, and help text. A baseline of 39 unverified
reasons is a gate that reads as coverage. It needs the classification pass
(dispatch vs test plan vs demo vs help text) first.

## Fix direction

Either resolve each list through the provider scan (`rmw_resolver::known_rmw_in`)
the way phase-439 W4 did for CMake, or classify the surface and extend
`check-rmw-agnostic`'s reach with a closed-list rule (two or more names in one
construct) once the noise is sorted.
