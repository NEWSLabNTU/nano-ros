---
id: 1620
title: "Four fixture-free nros-tests targets are red on main and no merge-gating
  lane runs them; one more hides a missing tool as a FAIL; fourteen report a
  missing fixture as a capability skip"
status: open
type: bug
area: testing, ci
severity: medium
found: 2026-10-01
related: [0584, 0922, 1226, 1610, phase-475]
---

## How these were found

phase-475's census: every `nros-tests` target run inside the gate lane's own
image (`ci/docker/ci-base`, built locally — the published package refuses an
anonymous pull), with the gate job's CLI build and no fixtures staged. Each red
below was then re-run on a normal host checkout, so none of them is an artifact
of the container.

They share one cause, and that cause is this issue: `test-unit` runs
`--workspace --exclude nros-tests`, so a target in this crate that needs nothing
reaches only the fixture lanes, and the fixture lane on `main` (`host-tests`) has
produced no green run in its last 200 (issue 1500). Issue 0922's defect,
measured: the exclusion is per crate and the property is per target.

## Red on `main`, fixture-free

| target :: case | what fails |
| --- | --- |
| `loc_budgets` | Zephyr shim 208 > 200 LoC — **issue 1610, fixed in #1514** |
| `example_portability::copies_within_a_group_are_identical` | `rust/listener [A-scheduled]`: `mps2-an385-freertos` and `native` differ from `esp32-c3-baremetal` — "make them identical, or add a KNOWN_DIVERGENCE entry naming the wave that will" |
| `no_local_axis_tables::no_matrix_axis_table_outside_matrix_and_interop` | `tests/qos_event_interop.rs:72  const QOS_EVENT_CELLS` — a matrix-axis table outside `src/matrix.rs` / `src/interop.rs`, which RFC-0051's single-matrix rule forbids |
| `params_per_node_interop::cases_bound_to_interop_cells` | the test's `#[case]`s cover `{(0,0,0,70)}`; `interop::CELLS` declares `{(0,0,0,70), (0,0,1,70)}` |

Not fixed here, each for the same reason: every one needs a decision about the
surface it guards (identical copies or a divergence entry; moving
`QOS_EVENT_CELLS` into the matrix or retiring it; a missing Cyclone case or a
stale cell), and the census can say *that* it is red, not which side is right.

The other three `params_per_node_interop` cases fail with `BuildFailed("no
[[fixture]] row for packages/testing/nros-tests/bins/param-two-node-talker …")`.
That is a fixture precondition rather than a fixture-free red, but **"no row"**
is not "not built": a bin with no manifest row is built by no lane at all. It is
likely the same omission as the `(0,0,1,70)` cell above.

## A precondition reported as a FAIL

`native_orchestration_misuse::launch_arm_resolves_the_bringup` fails with
`nros::main!: cannot resolve the SystemModel: nros-launch-resolve not found`.
The real gate job builds that resolver before this point, so this is not a red
on `main`. Two defects regardless:

* an unmet precondition ends in a hard FAIL instead of `skip!`, which CLAUDE.md
  forbids — the run cannot tell "the tool is missing" from "the code is wrong";
* the test runs `cargo` at test time (`Command::new("cargo")`, line 83), which
  CLAUDE.md also forbids ("No compilation inside tests"), and so needs the
  network to reach the crates.io index.

## A missing fixture reported as a capability skip — 14 targets

`advertised_state_interop`, `borrowed_e2e`, `custom_msg`,
`native_example_executor_bound_node_e2e`, `pool_exhaustion_threadx_linux`,
`px4_xrce`, `rmw_coordinate_truth`, `ros_editions_e2e`,
`ros_editions_nano_interop`, `rust_multi_node_per_node_graph`,
`sim_time_clock_e2e`, `xrce`, `xrce_ros2_interop`, `zephyr_leaf_staleness`.

(Seven more skips name something "not built" that is a TOOL or an IMAGE, not a
fixture — `nros-launch-resolve` ×3, the patched QEMU, the `ros_editions` jazzy
image ×3 — and are correctly capability skips.)

Each skips with `capability: … fixture not built` / `Test fixture binary not
prebuilt`. Issue 0584's `check-skip-budget` asserts that a missing fixture is a
hard failure and never a skip. The census set no lane scope, and 0584 records
that the class is invocation-dependent, so these may be correct for an
unscoped run — but fourteen is far more than the three laundering sites 0584
names, so either its scan is narrower than the sites, or these are fine and the
count should be explained. Not investigated past measuring it.

## Acceptance

The three remaining reds fixed or ruled, `native_orchestration_misuse` skipping
on its missing tool, and the fourteen either confirmed as 0584's known sites or
added to its scan. phase-475 W3 (admitting targets by census) waits on the reds,
so that admitting them does not turn the gate lane red for reasons no PR caused.
