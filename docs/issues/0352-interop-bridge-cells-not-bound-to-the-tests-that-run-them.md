---
id: 352
title: "Interop/Bridge matrix cells are not bound to the tests that run them: a cell's declared (lang, rmw) can silently disagree with the fixture its test builds"
status: open
type: bug
severity: medium
area: testing
related: [issue-0341, issue-0327, rfc-0051, rfc-0061]
---

## Finding (spun out of #341 defect 3, 2026-07-31)

`matrix_fixture_coverage.rs` proves every `Tier::Runtime` cell with a baked
fixture has a matching `examples/fixtures.toml` row at its `(platform, lang, rmw)`
coordinate. It **cannot** prove the same for `Kind::Interop` / `Kind::Bridge`
cells, and explicitly exempts them:

```rust
|| matches!(c.kind, Kind::Interop | Kind::Bridge)   // exempt: ros2 peers / bridge harness
```

The exemption is real, not lazy: an interop/bridge cell's peer is an ephemeral
process (a `ros2` node in a `DockerRosEnv`, a micro-XRCE Agent, an `rmw_zenohd`
router, a declarative bridge), and its nano side is often built **outside**
`fixtures.toml` — e.g. the `(ZephyrNativeSim, Rust, Zenoh, Qos, Interop)` cell's
nano entry is `build_zephyr_workspace_rust_qos_entry`, built by the west leaves
lane (`scripts/build/zephyr-fixture-leaves.sh`), and the native interop cells
(`ros_editions_e2e`) are built by `just ros_editions build-e2e-fixtures` into
`target-ros-edition-*`, never a fixtures.toml row. So there is no coordinate to
check them against.

**Consequence — the drift class #341 defect 2 was an instance of.** A cell can
declare `(ZephyrNativeSim, Cpp, Cyclonedds, Qos, Interop)` while the only test of
that shape (`qos_zephyr_ros2_interop_e2e`) boots the **Rust / Zenoh** entry. Both
directions wrong — a Cpp/Cyclonedds cell asserted covered by nothing, the real
Rust/Zenoh coverage unmodelled — and **nothing fails**, because no gate binds a
cell to the test that runs it. #341 defect 2 corrected that one cell by hand; the
class recurs on the next hand-added interop cell.

The phase-318 / RFC-0061 tier machinery (`ci_lane::{cells,coords}`) does **not**
close this: `coords(lane)` derives its `(platform, lang, rmw)` fixture coordinate
from the cell's *own* `lang`/`rmw` fields, so a cell that lies emits a lying
coordinate. The tier work reads the matrix; it does not cross-check the matrix
against test reality.

## Fix (the #341-deferred refactor)

Make the interop/bridge runtime tests **consume `matrix::CELLS` directly**, so a
cell's `(lang, rmw)` *drives* the fixture the test builds and the drift class
becomes unrepresentable — the same move #327 defect 2 names for the edition axis:

```
for cell in CELLS interop/bridge cells:
    let bin = build_for(cell);   // (platform, lang, rmw) select the builder
    run(cell, bin);              // no way to declare a cell the test does not run
```

Concretely:

1. A `build_for(cell)` seam that maps a `(platform, lang, rmw, workload)`
   coordinate to its fixture-builder (zephyr west entry / native example /
   docker-peer nano bin), replacing the by-hand `build_zephyr_workspace_rust_qos_entry`
   / `example_of` calls scattered across the interop test files.
2. Parametrize `qos_zephyr_ros2_interop_e2e`, `interop_e2e`, `ros_editions_e2e`,
   and the `declarative_bridge_*` tests over the Interop/Bridge cells of
   `matrix::CELLS` (rstest `#[case]`s generated from, or asserted against, the
   matrix — not local `enum Workload`/`enum Dir` copies as `ros_editions_e2e`
   does today).
3. Once every Interop/Bridge Runtime cell is provably run by exactly one
   parametrized case whose builder is chosen by the cell's own fields, drop the
   blanket `Kind::Interop | Kind::Bridge` exemption in
   `matrix_fixture_coverage.rs` (or replace it with a "every interop cell is
   claimed by a case" assertion).

Scope note: larger than a gate tweak because the three build mechanisms differ
(west entry vs native example vs ephemeral docker peer) and each test file wires
its peer differently. That is why #341 shipped defects 1+2 and spun this out.

## Relationship to #341 / #327

- #341 defects 1+2 (uORB expressible, zephyr QoS cell corrected) — DONE (2fabffd33).
- This issue is #341 defect 3, extracted so #341 can close.
- Shares its "make the runtime test consume the matrix SSoT" fix shape with #327
  defect 2 (the edition axis); doing either first de-risks the other.
