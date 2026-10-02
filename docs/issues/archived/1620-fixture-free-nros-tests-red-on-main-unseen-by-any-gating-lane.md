---
id: 1620
title: "Four fixture-free nros-tests targets are red on main and no merge-gating
  lane runs them; one more hides a missing tool as a FAIL; fourteen report a
  missing fixture as a capability skip"
status: resolved
type: bug
area: testing, ci
severity: medium
found: 2026-10-01
resolved: 2026-10-03
related: [0034, 0501, 0584, 0922, 1025, 1129, 1226, 1452, 1610, 1656, phase-475]
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

## 2026-10-02 — the three reds are fixed; what was wrong was different in each

**Each was a different side going stale, so each fix went the other way.**

| red | stale side | fix |
| --- | --- | --- |
| `no_local_axis_tables` | the TEST — `qos_event_interop.rs` kept its coordinate in a named `QOS_EVENT_CELLS` table | stated inline at `assert_test_bound`, as every sibling interop test does; the gate was right |
| `params_per_node_interop::cases_bound_to_interop_cells` | the TRIPWIRE — issue 1268 added the Cyclone cell and its case, not the tripwire row | `(Linux, Rust, Cyclonedds, Params)` added |
| `example_portability` | **three groups, not one** — the failure listed several and the first read stopped at the first | see below |

**`params_per_node_interop` also had a second, deeper break** the census showed
as `no [[fixture]] row … Selector { rmw: "" … }`. `b6055b6f3` (#1268) gave the
zenoh row `rmw`/`no_default_features`/`features` so it mirrors its Cyclone
sibling, and `build_native_param_two_node_talker()` kept selecting `plain()` —
which then matched NO row, so both zenoh cases failed in every lane, fixtures
or not. The builder now selects `rmw(Zenoh)`, the same rule as its sibling.
Swept: every other `plain()` site (25 resolutions, including the 20 literal
callers of `build_example` / `build_test_fixture_at_profile`) has a matching
plain row.

**`example_portability` had three divergences:**

* `rust/listener` and `rust/talker` on **esp32-c3** — `eb95b378a` (#1265, a day
  earlier) switched their two log calls to `nros::log_info!` while target-scoping
  the board deps. The switch was incidental: the manifest still declares `log`
  as a host-buildable dependency and still documents it as the console path, so
  the esp32 copies went back to `log::info!`, the group's form in seven of eight.
* `rust/service-client` — `c27a9601f` (#1087, 09-06) added rclcpp's
  `wait_for_service` gate (`service_is_ready_for_name`) to the **native** copy
  only. Here native was the deliberate improvement, so it went the other way:
  the guard was propagated to all five other group copies. It is safe on every
  backend by construction — `Err` means "the backend cannot say" and the
  request goes out as before (Cyclone and XRCE leave the slot NULL) — and the
  six C/C++ wait loops already rely on the same `service_is_ready` on every
  platform. **This test had been red since 09-06**, which is how long a
  fixture-free test can stay red when no merge-gating lane runs it.

Acceptance, built rather than read: `freertos rust` and `threadx-linux rust`
fixture families built, and both `service-client` images (thumbv7m and
ThreadX-linux) carry the guard's string; the esp32 talker and listener
cross-built for `riscv32imc` (that family's later **packing** step fails with
`nros_fixture_row_artifact_dir: command not found` — a function the make
driver's subshell cannot see, unrelated to these files, not investigated). The
NuttX, rv-virt-ThreadX and Zephyr copies were not built here (no provisioned
SDK); they share the edited `lib.rs` shape and the `TickCtx` API that the two
built images type-checked.

## Still open

* `native_orchestration_misuse` — a missing tool reported as a FAIL, and a
  compile at test time.
* the 14 fixture-as-skip targets.
* **new**: in a full-crate parallel run inside the gate image,
  `cmake_platform_matrix` and `native_main_macro_misuse` fail, and both pass in
  isolation in BOTH image variants with identical source. Both compile at test
  time; they contend with each other under parallelism. phase-475 W3 must not
  admit a test that compiles at run time until it runs isolated or is fixed.

## 2026-10-03 — the four "Still open" items are closed; residue moved to issue 1656

### 1. `native_orchestration_misuse` — deleted: it compiled a fixture another row already builds

`launch_arm_resolves_the_bringup` staged `orchestration_tiers_native`, wrote
`nros::main!(launch = "demo_bringup");` into `demo_entry/src/main.rs`, and ran
`cargo check`. That line is what the fixture already COMMITS (issue 0438 made the
launch arm mandatory there), and the `orch_tiers_multi` compile-check row already
`cargo build`s the fixture verbatim. So the test was a second, run-time compile
of a build-stage artifact. It is deleted. Its intent now lives in
`native_orchestration_tiers::launch_arm_resolves_the_bringup`, which reads the
prebuilt `orch_tiers_multi` binary through `require_compile_check_bin`. It also
pins the fixture's one `nros::main!` line to the launch arm: a fixture that slid
to `model =` would keep every other assertion in that file green.

Item (a), "skip when `nros-launch-resolve` is missing", is gone rather than
patched, because the test needs no tool now. What can fail is a missing fixture,
and in a gated run a missing fixture is a hard failure (issue 0584). That is the
correct verdict.

### 2. Must-fail compiles moved to the build stage as VERDICT rows

The registry kept must-fail compiles at test time because "a fixture whose
configure fails fails the BUILD". That holds when the artifact is the compiled
output. It does not hold when the artifact is the compile's **verdict**.

`scripts/build/compile-check-fixtures.sh` gains two builders,
`cargo-check-verdict` and `cmake-configure-verdict`:

* They stage the tree like `cargo-check` and run the compile.
* They record `exit=` plus stdout and stderr (`.verdict`, `verdict.*`).
* They succeed whatever the compiler said. An infrastructure failure still fails
  the build: no source, a refused sync, or no cmake.
* The test asserts the verdict through
  `nros_tests::fixtures::require_compile_verdict`, so the assertion stays in the
  test.

The new builders back 14 rows:

* **`n9_workspace` overlays (5):**
  * three `main_macro_misuse_*` rows;
  * `main_macro_resolves_from_inputs`, staged with no sync and with
    `NROS_MODEL_DIR` unset;
  * `main_macro_rebuilds_on_model_touch`, which compiles twice across a rewrite
    of the synced model and records the first compile as `verdict.prelude.*`.
* **`diagnostic_*_verbatim` (2).**
* **`cmake_platform_threadx_requires_board` (1).**
* **`cmake_register_*` (6),** one fixture whose cases live in `cases/<id>.cmake`
  files.

**The move had to solve two staleness problems:**

* **A FAILED configure writes no dependency record.** It leaves neither
  `CMakeFiles/Makefile.cmake` nor `build.ninja`, so the signature's measured
  closure was empty for exactly these rows. An edit to the module under test
  (`cmake/NanoRosNodeRegister.cmake`) would then have left a museum verdict
  reading FRESH.
  * Fix: the configure runs with `--trace-format=json-v1`, and
    `scripts/build/cmake-trace-deps.py` writes the traced listfiles as a
    `verdict.d`, which `dep-closure.py` already reads.
  * Measured on `cmake_register_unqualified_class`: 22 in-repo cmake modules in
    the closure, `NanoRosNodeRegister.cmake` among them.
* **A verdict row never fails its own build, so a row no test reads reports
  nothing.** This is issue 1032's shape for `cxx-syntax`. `fixtures-manifest.py`
  validation now refuses a verdict row with no consumer, and refuses one whose
  `output` is not `.verdict`.

**Converted:**

* `native_main_macro_misuse` (5 tests) and `cmake_platform_matrix`: the pair
  that failed together in a parallel run.
* `cmake_node_register_misuse` (4) and `diagnostic_verbatim` (2): same shape,
  same builders.
* Each was removed from `negative_diagnostic_registry.rs`.
* The two `.config/nextest.toml` timeout overrides that existed only for these
  compiles were removed too. A stale `binary()` is a parse error (issue 0743).
* `check-lane-contracts` lists `require_compile_verdict` as a COMPILE-stage
  resolver. Its `require_*` harvest would otherwise have classed it as RUNTIME.

**Sweep, for the next person:**

```
git grep -n 'Command::new("cargo")\|Command::new("cmake")\|"west"' packages/testing/nros-tests/tests/
grep -l 'cargo build\|cargo check\|cmake -S\|cmake -B\|cmake --build' packages/testing/nros-tests/tests/*.sh
```

**Issue 1656** lists every hit not converted here, with why it stays:

* `platform_header_compile`'s negative cell needs a `cxx-syntax` verdict builder.
* `zpico_drift_gate` sandboxes the canonical tree.
* The rest are relative checks, soaks, seam probes, or not compiles at all.

**Measured** in this worktree, with the verdict rows built via
`NROS_FIXTURE_BUILDER=cargo-check-verdict,cmake-configure-verdict`:

* All 14 rows were recorded, and the stale probe reads all 14 FRESH.
* Cold cost per row: about 15 s and 475 MB for a cargo row, about 2.6 s for a
  cmake row.
* Each touched target passed solo and in one parallel `cargo nextest` run: 20/20
  tests across six targets in 6.3 s, including the boot tests in
  `native_orchestration_tiers`.
* `native_main_macro_misuse` now takes 0.09 s. Phase-342 W2 measured it at
  10–108 s.

### 3. The 14 fixture-as-skip targets: correct for an unscoped run, except where a site bypassed the shared funnel

`check-skip-budget` asserts over a GATED run's junit. The census set no lane
scope, and in an unscoped run (`NROS_TEST_COORDS` unset) a missing fixture is a
skip by design. So the real question was which sites would still skip in a gated
run.

That was measured by re-running the 14 with `NROS_TEST_COORDS` set to every
manifest coordinate. Two groups came out.

**Nine are correct.** They go through a `build_*` resolver's `.require()`, which
panics `MISSING for an in-lane coordinate` once gated:

* `advertised_state_interop`, `custom_msg`, `native_example_executor_bound_node_e2e`
* `pool_exhaustion_threadx_linux`, `rust_multi_node_per_node_graph`
* `xrce`, `xrce_ros2_interop`
* `px4_xrce`, which takes a proper `[SKIPPED:lane]`

**Five, plus two the sweep found, bypassed the funnel.** Each probed `is_file()`
itself and then called `skip!`, which skips in every run. That is 0584's
laundering with no resolver call for a scan to see:

* `sim_time_clock_e2e` caught every resolver error with `unwrap_or_else(skip!)`,
  stale fixtures included.
* `ros_editions_nano_interop`.
* `ros_editions_e2e`, through `ros_env::e2e_setup*`.
* `zephyr_leaf_staleness`, at 3 sites.
* `rmw_coordinate_truth`, whose row loop did `continue // not built`.
* `wake_latency_cortex_m3`.
* `borrowed_e2e` (see below).

**The fix is one funnel, not seven spellings:**

* `require_prebuilt_artifact(path, remedy)` does the lane check, then gives the
  same absent-fixture verdict every resolver gives.
* `lane::absent_row_breaks_promise` is the row-loop counterpart.
* `rmw_coordinate_truth` now fails when a gated run promised rows and located none.

**The scan was narrower than the sites,** so it was widened by deriving the
subject rather than adding a list:

* `check-fixture-require` now also counts the CODE SHAPE
  `if !<path>.is_file()/exists()/is_dir() { … skip!(…) }`.
* It keys on shape, not wording, because wording is how issue 1129's scan missed
  sites.
* The count is a ratchet that can only shrink:
  `.config/fixture-existence-skip-baseline.txt`, 12 sites in 9 files, mostly real
  capability probes.

**`borrowed_e2e` keeps its own probe, deliberately.** Its stamp is produced only
by its own gate recipe; no fixture lane builds it. Routing it through the funnel
would cause two failures:

* `check-lane-contracts` refuses it, because the funnel is a RUNTIME resolver and
  `ci gate` reaches this test.
* A gated `test-all` would fail on an artifact nothing in that lane produces.

Issue 1656 records it: give it a row.

### 4. esp32 packing — `nros_fixture_row_artifact_dir: command not found`

The failing step was not the espflash pack. It was the stack-floor check
`nros_fixture_check_stack_floor`, which `fixtures-build.sh` runs after each row
inside a make LEAF:

* The leaf is a fresh bash with only what `export -f` shipped, and the resolver
  list left out `nros_fixture_row_artifact_dir`.
* It reproduced only for a family build. With one row (`--id …`), `run` stays
  serial and in-process, so the function was visible.
* Fix: the function joins the existing `export -f` list, and stays in its one
  home, `fixtures-target-dir.sh`.

`check-export-f-closure`, the gate for exactly this class (issues
0400/0706/0712), missed it:

* It read only column-0 function definitions, and both leaf functions are
  defined four spaces in, inside an `if`.
* It now accepts indented definitions.
* Whole-line comments are now dropped before calls are harvested. A defined
  function named only in a comment was the false positive the widening exposed.
* Two self-test cases cover both changes.

Measured on `scripts/build/fixtures-build.sh esp32 rust`:

* **Before:** rc=2, with three `command not found`.
* **After:** rc=0. `esp32_qemu_listener` (75,696 B), `logging-smoke-esp32-qemu`
  and `esp32_qemu_talker` each pass the 32,768 B floor.
* **Negative control:** with the export removed, the widened gate FAILS, naming
  the function and its caller.
* `check-fixture-artifact-dir-inputs`: OK.

One gap remains: the gate scans `scripts/build/*.sh` only. The `export -f` lists
in `justfile`, `just/native.just` and `scripts/debug/debug-keyexpr.sh` are
closed by hand today and are outside it.

### Verification

* `just check fast`: 369 gates ran, 0 failed, 16 skipped. The skips are this
  host's environment: Zephyr not provisioned, NuttX FFI submodules not checked
  out.
* `just test-lane-contracts`: 30/30.
* `check-lane-contracts --selftest`: 60/60.
* `fixtures-manifest.py validate-compile-checks`: 56 rows.
* `cargo clippy -p nros-tests --tests -D warnings`: clean.
* `cargo +nightly fmt --check`: clean.
* `just ci gate` passed (compile + unit, no fixtures). The tier is that gate plus
  the targeted fixture builds above. This is not tier 1: no `build-test-fixtures`
  sweep was run.
