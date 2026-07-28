---
id: 327
title: "The ROS-edition axis sits outside the test matrix: Cell has no edition field, five per-cell ros_editions files bypass RFC-0051, and ARCHITECTURE §2 still calls jazzy 'planned'"
status: open
type: bug
severity: medium
area: testing, docs
related: [rfc-0051, rfc-0056, rfc-0058]
---

## Finding (audit 2026-07-28, P2)

ARCHITECTURE §2 declares three axes (RMW × platform × ROS edition). Two of them
are matrix-derived per RFC-0051; the third is not, and the consequences chain.

### 1. `Cell` has no edition field

`packages/testing/nros-tests/src/matrix.rs:223` — the cell type carries platform,
lang, RMW and workload. The ROS edition is instead read from an
`NROS_ROS_EDITION` env var (`ros_env::test_edition()`), so the declared third
axis is structurally outside the table that is supposed to enumerate the
supported space.

### 2. Which is why the newest family bypasses RFC-0051

`packages/testing/nros-tests/tests/ros_editions_zenoh.rs:40-190` plus
`ros_editions_{e2e_pubsub,e2e_service,e2e_action,xrce}.rs` are **five
hand-written per-cell files with 16 near-identical `#[test]` bodies**, none
consuming `matrix::CELLS` — exactly the shape phase-295 retired for every other
runtime lane. The three `e2e_setup{,_xrce,_zenoh}` helpers are already the
per-RMW seam, so the bodies differ only by the setup call and the marker
constant.

This is not merely stylistic: the same absence of a matrix row is why the six
inline marker literals in issue #321 had no gate covering them.

### 3. ARCHITECTURE §2 is stale about the axis it declares

§2 still lists supported editions as `ros-{humble,iron}` with "jazzy/rolling
planned", but **jazzy is the delivered default** — `just/ros-editions.just:13,111`
(`distro="jazzy"`), a real `ros-jazzy` cargo feature
(`packages/core/nros/Cargo.toml:141`, `packages/zpico/nros-rmw-zenoh/Cargo.toml:110`),
and it is the only edition the zenoh interop lane can structurally run.

The real carve-out is recorded **only as a code comment** at
`packages/testing/nros-tests/src/ros_env.rs:957-962`: humble and iron ship no
`rmw_zenoh_cpp` apt package, so those two zenoh cells are permanently skipped.
`examples/README.md`'s coverage matrix — which is otherwise strong, with an
explicit 9-row carve-out table and lift conditions — has no edition column at
all.

## Fix

1. Add `pub edition: Edition` to `Cell` (or, if edition is genuinely a per-run
   global rather than a cell axis, say so in a doc comment in `matrix.rs` and in
   ARCHITECTURE §2 — the current silence is what let the axis drift).
2. Collapse the five per-cell files into one parametrized `rtos_e2e.rs`-shaped
   rstest over (rmw, workload, direction).
3. Promote jazzy to supported in ARCHITECTURE §2; move the humble/iron
   `rmw_zenoh_cpp` carve-out out of `ros_env.rs` into the coverage matrix as a
   documented row.
4. Add an edition column (or explicit carve-out) to `examples/README.md`.

## Deliberately out of scope

The ~40 older per-cell `realtime_tiers_*_e2e.rs` / `*_entry_e2e.rs` files share
the E6 shape but are pre-existing debt that phase-295 W3 is still migrating;
they are not part of this issue.
