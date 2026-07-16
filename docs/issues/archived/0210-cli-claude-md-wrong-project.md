---
id: 210
title: "packages/cli/CLAUDE.md is the retired colcon-cargo-ros2 guide — describes a different project"
status: resolved
type: tech-debt
area: docs
related: []
---

## Problem (audit 2026-07-16, H1)

`packages/cli/CLAUDE.md` is wholesale the old "colcon-cargo-ros2:
Development Guide" (rclrs `user-libs/`, PyPI wheels, dual-workspace, colcon
extension) — none of which describes the in-tree `packages/cli/`
(cargo-nano-ros / nros-cli-core / nros-cli sub-workspace). Every agent
session that touches the CLI loads misleading instructions.

## Fix sketch

Rewrite for the actual sub-workspace: build via `just setup-cli`, the three
crates' roles, the nested submodules, codegen/orchestration entry points.

## Resolution (2026-07-16)

Rewrote packages/cli/CLAUDE.md for the in-tree sub-workspace: build via
`just setup-cli` + stale-shadow warning, fixture-staleness coupling, version
lockstep, crate map (all 11 members + the colcon extension + nested
submodules), and the "nros is a generic tool / index-driven" design rules.
