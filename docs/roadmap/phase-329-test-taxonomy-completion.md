# Phase 329 — Test taxonomy completion: every test under a matrix, every build in the build stage

**Implements:** RFC-0051 (the unfinished half), RFC-0058 (edition axis stays
separate), RFC-0061 (tier mapping)
**Informed by:** the 2026-08-01 full-tree inventory (156 test files, 454 test
fns, ~587 executed cases) and the first complete `test-all` triage on a
product-provisioned host (issues 0380–0382).

## Problem

RFC-0051 declared ONE generated matrix; phase-295 built the machinery
(`matrix::CELLS`, `interop::CELLS`, `alloc`, `StandardChecker`,
`matrix_fixture_coverage` G1–G4) — but the tree has regrown around it:

1. **Only 3 of 156 files iterate a CELLS list** (`matrix_fixture_coverage`,
   `zephyr.rs` via `runtime_cells()`, `interop_e2e` via `test_covers`). The
   five files that call themselves "THE matrix consumer" — `entry_e2e`,
   `realtime_tiers_e2e`, `multihost_e2e`, `roundtrip_xprocess_e2e`,
   `workspace_features_e2e` — hand-list `Cell{…}` literals in `#[case]`
   tables. Adding a Runtime row to `CELLS` adds NO case anywhere except
   zephyr.rs (and zephyr.rs only asserts the COUNT matches).
2. **62 PLATFORM-E2E files** hardcode their (platform, lang, rmw) coordinate
   outside any matrix — the pre-295 shape growing back (native workload
   files, per-platform QEMU files, per-family one-offs).
3. **The realtime-dim family (14 files)** is its own implicit matrix
   (dim × platform × lang) with no table: `*_core_pin_applied` ×5,
   `zephyr_edf_deadline_applied`, `threadx_{preempt_threshold,time_slice}`,
   `nuttx_{sporadic_budget,tier_priority}`, `realtime_tiers_e2e`,
   `tier_filter`, `native_orchestration_tiers`, `orchestration_tiers_freertos`.
   Issue 0380 proved the cost: when regeneration stripped the model dims,
   nothing structural said which (dim × platform) cells existed to lose.
4. **The ROS-interop surface is split**: 6 files bind to `interop::CELLS`;
   6 more talk to a live ROS 2 peer with NO binding
   (`bridge_zenoh_to_cyclonedds` — the only unbound bridge e2e —, `params`
   ros2-param lanes, `qos_override_e2e`, `cpp_multi_node_entry`,
   `rust_multi_node_per_node_graph`, plus the RFC-0058 `ros_editions_*`
   docker axis which is deliberately separate).
5. **7 files compile or link at test time beyond the sanctioned
   configure-FAIL exception** (worst: `zpico_drift_gate` runs a full
   `cargo build` of a C-heavy sys crate per test run). The E1 rule
   ("no compilation inside tests") holds only because nobody looks.
6. Duplicate machinery: `platform_header_matrix.rs` defines a SECOND local
   `const CELLS`; `examples_canonical_shape.rs` and `example_shape.rs` are
   two independent shape walkers over `examples/`.

## Target structure

Every test file belongs to exactly ONE of these buckets, and the bucket is
visible from the file's location/consumer, not tribal memory:

| bucket | table it consumes | runtime? |
| --- | --- | --- |
| Cell matrix (platform × lang × rmw × workload × kind) | `matrix::CELLS` | boots prebuilt fixtures |
| ROS-interop matrix (nano cell + peer + direction) | `interop::CELLS` | live ROS 2 peer |
| ROS-edition matrix (edition × rmw × workload × dir) | `ros_editions_e2e` cases (RFC-0058; edition is a per-run global, NOT a Cell field) | docker peer |
| Realtime-dim matrix (dim × platform × lang) | NEW `matrix::sched_dims::CELLS` | boots ws-realtime fixtures |
| CLI-behavior suite | none (TempDir staging) | host only |
| Fixture-artifact checks | `fixtures.toml` rows | no boot |
| Guards/gates | none | host only |
| Host-unit | none | host only |
| Negative-diagnostic registry | NEW explicit list (see W5) | cmake/cargo FAIL-path only |

## Work items

### W0 — RFC-0051 amendment (small)
- [ ] Add the **realtime-dim matrix** and the **negative-diagnostic
  registry** to RFC-0051 as first-class structures; record the resolution
  of open question 2 (Mixed stays a `Lang`) as implemented.

### W1 — close the consumer loop (the 2026-08 defect)
- [ ] `entry_e2e`, `realtime_tiers_e2e`, `multihost_e2e`,
  `roundtrip_xprocess_e2e`, `workspace_features_e2e`: derive the `#[case]`
  list FROM `matrix::CELLS` (rstest over a `const` filter of CELLS — the
  zephyr.rs pattern, but iterating, not counting). A new Runtime row must
  fail G1 until a case exists, and must RUN once the fixture row lands.
- [ ] Gate: `matrix_fixture_coverage` G5 — every file whose header claims a
  matrix role derives its case set from CELLS (assert set-equality between
  the file's inventory and the CELLS filter it declares; count-only
  assertions like zephyr.rs L671 upgrade to set equality).
- [ ] Kill the second spelling: `platform_header_matrix.rs` local `CELLS`
  table rewritten over `matrix::PlatformId` (its cells move to the
  compile-check fixture family in W5).

### W2 — realtime-dim matrix
- [ ] `matrix::sched_dims` — `SchedDim {CorePin, EdfDeadline,
  PreemptThreshold, TimeSlice, SporadicBudget, TierPriority}` ×
  platform × lang table with per-cell `Expect {KernelAccept, FailLoud,
  CarveOut(reason)}` — the RFC-0052 fail-loud contract becomes data.
- [ ] The 10 single-cell `*_applied` files fold into ONE
  `sched_dims_applied.rs` rstest consumer (markers already centralized in
  `output.rs`); `realtime_tiers_e2e` keeps the delivery-scheduling cases
  and joins the same table for its cell list.
- [ ] Bake-time gate (issue 0380): a build-stage check that every dim the
  table expects for a (platform, lang) cell is present in the committed
  ws-realtime model — a stripped model fails the BUILD, not a QEMU e2e.

### W3 — ROS-interop matrix completion
- [ ] Bind the 5 unbound live-peer files to `interop::CELLS` rows
  (`bridge_zenoh_to_cyclonedds` first — it is the G4 blind spot;
  then `params` ros2-param lanes, `qos_override_e2e`,
  `cpp_multi_node_entry`, `rust_multi_node_per_node_graph`).
- [ ] The peer requirement becomes part of the cell (`peer:` already
  exists) so `just test-all` on a ROS-less host reports these as ONE
  skipped matrix, not 50 scattered skips.
- [ ] `ros_editions_*` stays a separate matrix per RFC-0058 — document the
  boundary in interop.rs (already half-written in its docs).

### W4 — PLATFORM-E2E fold-in triage
Not a mass rewrite — a dispositioning pass with the same rule phase-295
used: fold when the file is a per-cell duplicate of a matrix workload;
keep (and label) when it tests a genuine one-off behavior.
- [ ] Native workload files that duplicate matrix workloads fold into the
  W1 consumers (`services`, `actions`, `qos`, `multi_node`, `custom_msg`,
  `zero_copy`, `error_handling`, `executor` → Workload rows; `native_api`'s
  28 rstest cases become the Rmw-parametrized native consumer).
- [ ] Per-platform QEMU files that duplicate `rtos_e2e` workloads fold in
  (`freertos_qemu`, `nuttx_qemu`, `threadx_linux`, `threadx_riscv64_qemu`,
  `c_riscv_nuttx_e2e`, parts of `emulator.rs`/`esp32_emulator.rs`).
- [ ] Everything kept gets a one-line header naming its bucket + why it is
  not a cell (the E5 carve-out rule, applied to files).
- [ ] Target from RFC-0051 restated: tests/ file count drops ~40 while
  CELLS coverage goes UP.

### W5 — build checks move to the fixture build stage
The E1 rule gets an enforcement surface. Sanctioned at runtime: FAIL-path
diagnostics only (a configure/compile that MUST fail cannot be a passing
prebuilt fixture).
- [ ] `zpico_drift_gate` → `compile_check_fixture` row (the cargo build
  moves to `scripts/build/compile-check-fixtures.sh`); test asserts the
  artifact + drift metadata.
- [ ] `platform_header_matrix` positive-compile cells → compile-check
  fixture family generated from the matrix vocabulary; the FAIL-path cells
  stay runtime (registry below).
- [ ] `cross_libc_precedence_gate` positive compile → build stage.
- [ ] `staticlib_duplicate_symbols` link proof → build stage (link once at
  fixture build; test runs `nm` on the artifact).
- [ ] `cli_bringup_nuttx` `make context` → prebuilt into the nuttx fixture
  stage; test consumes the staged tree.
- [ ] **Negative-diagnostic registry**: one module (or fixtures.toml
  section) listing every sanctioned runtime FAIL-path invocation —
  `cmake_node_register_misuse`, `cmake_platform_matrix`,
  `native_main_macro_misuse`, `native_orchestration_misuse`,
  `diagnostic_verbatim` — with the tool it invokes and why it cannot be
  prebuilt. A gate greps `tests/` for `cmake`/`cargo`/`cc`/`gcc`/`make`
  invocations and fails on any file NOT in the registry (the 0196 rule:
  the gate covers the class, not the known sites).

### W6 — gate dedup + marker sweep
- [ ] Merge `examples_canonical_shape.rs` into `example_shape.rs` (one
  walker).
- [ ] `output_marker_gate` extends to the 7 files parsing runtime node
  output with bespoke greps (`logging_smoke`, `nuttx_qemu`, `platform`,
  `qos_zephyr_ros2_interop_e2e`, `rust_multi_node_per_node_graph`,
  `fvp_runtime_ws`, `qos_override_e2e`) — either their markers join
  `output.rs` or the file documents why not.
- [ ] Grep-gate against new local `CELLS`/axis tables outside
  `matrix.rs`/`interop.rs`.

### W7 — tier mapping (RFC-0061 join)
- [ ] Each bucket declares its tier home: guards/gates + host-unit +
  CLI-behavior + fixture-artifact = tier 1; cell matrix 1-wise = tier 2;
  pairwise + interop + realtime-dim full = nightly; editions + full
  matrix = tier 3. Today the mapping exists only as lane filters; write it
  into the tables so `ci_lane` selection reads it.

## Acceptance

- Adding a Runtime cell to `matrix::CELLS` with no test change fails G1;
  adding the fixture row makes it RUN in the right consumer — demonstrated
  once in CI as part of landing W1.
- `grep -rn 'cargo build\|cmake -S\|cc \|g++\|make -C' tests/` hits only
  registry members (W5 gate green).
- tests/ file count ≤ 120 (from 156) with CELLS-derived case count ≥
  today's ~587.
- A ROS-less host's `just test-all` reports skips grouped by matrix
  (interop / editions), not per-file.

## Sequencing note

W1 and W2 touch `realtime_tiers_e2e` — land after the issue-0380 model
restoration settles (this phase's W2 gate is 0380's structural fix).
W4 is the long tail; it can proceed cell-family by cell-family behind the
W1 gates without a flag day.
