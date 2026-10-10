---
id: 1650
title: "Example trees that no lane builds — they can rot with every gate green"
status: resolved
type: tech-debt
area: examples, ci, testing
severity: medium
found: 2026-10-03
related: [1521, 1627, 1453, phase-470, phase-477]
---

## What this is

A census of all 192 example roots on `main` (phase-477; method below) joined each
root's on-disk shape to the build road the fixture manifest DECLARES for it
(`[[fixture]]`, `[[workspace_fixture]]` and `[[compile_check_fixture]]` rows,
via `fixtures-manifest.py`'s `row_builder()`). Most roots have a row. These do
not, and nothing else compiles them:

| tree | what reads it today | built by |
| --- | --- | --- |
| `examples/workspaces/launch` — the launch-v1 composition demo (`<group ns>`, `<include>`, `$(var)`, remaps) | `example_shape.rs` checks its SHAPE | nothing |
| `examples/templates/multi-package-workspace` | `scripts/check-msg-dep-is-path.sh` lints its manifests | nothing (`native.just`'s example sweep EXCLUDES it by name) |
| `examples/templates/zephyr-byo` | `check-zephyr-module-allowlist.py` reads its west manifest | nothing |
| `examples/templates/multi-node-workspace` | `cargo-nano-ros`'s scaffold `include_str!`s its `.colcon_workspace`; one CLI test reads its contract | no fixture row; `native.just`'s sweep excludes it by name |

`examples/bridges/rust/tt-zenoh-to-{cyclonedds,xrce}` were the other two. They
were deleted (maintainer decision, 2026-10-03) rather than given rows: the
bridge e2e tests had always resolved separate `bins/bridge-*-fwd` fixtures, and
the resolver's static was misnamed as if the examples were the fixture.

## Why it matters

An example no lane compiles can break with every gate green, and an example is
exactly what a user copies. This is the same unreported-lane class as issue
1521 one level up: 1521 was tests no merge-gating lane RAN, this is examples no
lane BUILDS at all.

## What to do

Per tree, decide by measurement, not by type:

1. **Can it build in a lane?** Give it a row — build-only is enough (a
   `[[compile_check_fixture]]` configure, or a cargo/cmake `[[fixture]]`).
   `launch` is the strongest candidate: it is a full workspace, and it is the
   only end-to-end exercise of launch v1 composition in the tree.
2. **Is it deliberately not a build target?** `zephyr-byo` ("bring your own"
   west manifest) may be documentation of a shape rather than something to
   build. Then say so where a reader looks, and keep only the lint.
3. **Is it superseded?** Delete it, as with the TT bridges.

Then close the class with a gate: **every example root has a row, or a recorded
reason it does not** — keyed on the census's shape detection, not on a list of
paths (the issue-0196 rule). It needs a home in a lane that runs
(`check-default-gates-run-somewhere`) and a selftest (`check-gate-selftests`).

## Census method (re-runnable)

Walk `git ls-files examples`; a root is `examples/<plat>/<lang>/<name>`,
`examples/workspaces/<ws>` or `examples/templates/<t>`. Load the manifest with
`fixtures-manifest.py`'s `load` / `load_workspace_fixtures` /
`load_compile_check_fixtures`; a root is covered when any row's `dir` equals it
or lies under it. The script used is recorded in phase-477 W2.

## Resolved (2026-10-10, phase-477 D3)

Each tree decided per D3, and the class closed with a gate:

| tree | outcome |
| --- | --- |
| `examples/workspaces/launch` | row `workspace-rust-native-launch` (`[[workspace_fixture]]`, built IN PLACE — its node packages reach `nros` by an in-repo path, so a staged copy cannot resolve them); builds in 45 s. Same `(linux, rust, zenoh, workspace)` cell as `workspace-rust-native`, so `matrix_fixture_coverage` needs no new cell |
| `examples/templates/multi-node-workspace` | row `multi_node_workspace_rust` (`compile_check_fixture`, staged `nros sync` + `nros build`) → `native_entry` |
| `examples/templates/multi-package-workspace` | row `multi_package_workspace` → `pkg_rust_publisher` (the same build compiles its C and C++ packages). It was the ONE workspace template with no tracked `.colcon_workspace`, so the builder read it as a plain cmake project and failed on the missing root `CMakeLists.txt` — the marker is added |
| `examples/templates/zephyr-byo` | recorded reason: documents the bring-your-own-west-manifest SHAPE, not a build target; `check-zephyr-module-allowlist` lints its `west.yml` |

The census also found five roots the original table did not list, each now
with a recorded reason: the two `esp32-c3-baremetal` Rust leaves (ESP32 is
dormant by maintainer decision — issue 1525 deleted their rows on purpose) and
the three `examples/px4` roots (built by their own `just px4 build-fixtures`
recipe against an external PX4-Autopilot tree, not by a manifest row).

**Gate:** `just check example-build-coverage`
(`scripts/check-example-build-coverage.py`, fast line, ~0.1 s). Roots are found
by SHAPE from `git ls-files examples`; the manifest is read through
`fixtures-manifest.py`'s own loaders; a root is covered when a row's `dir`
equals it or lies under it. Uncovered roots need a reasoned line in
`.config/example-build-coverage-baseline.txt`, ratcheted both ways. Selftest
on the normal path (root shape; a row under a root covers it; a sibling with a
common prefix does not). Today: 188 of 194 roots built by a row, 6 with a
reason.
