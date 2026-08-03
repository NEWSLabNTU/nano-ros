# Phase 329 — Test taxonomy completion: every test under a matrix, every build in the build stage

**Implements:** RFC-0051 (the unfinished half), RFC-0058 (edition axis stays
separate), RFC-0061 (tier mapping)
**Informed by:** the 2026-08-01 full-tree inventory (156 test files, 454 test
fns, ~587 executed cases), the first complete `test-all` triage on a
product-provisioned host (issues 0380–0382), and a 2026-08-04 4-agent full-tree
re-sweep that added the build-cost surface (344 fixture build rows) and W8
(see "Validated inventory" + W8 below).

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
- [x] Add the **realtime-dim matrix** and the **negative-diagnostic
  registry** to RFC-0051 as first-class structures; record the resolution
  of open question 2 (Mixed stays a `Lang`) as implemented.

### W1 — close the consumer loop (the 2026-08 defect)
- [x] `entry_e2e`, `realtime_tiers_e2e`, `multihost_e2e`,
  `roundtrip_xprocess_e2e`, `workspace_features_e2e`: derive the case list
  FROM `matrix::CELLS`. **Landed 2026-08-04.** Each file now iterates
  `CELLS.filter(|c| w1_consumer_of(c) == Some(<self>) && Runtime)` in ONE
  `#[test]`; the local `Cell{}` tables became `Exec` (execution data only),
  keyed by coordinate via `exec_for` (an unmapped claimed coordinate is a HARD
  panic, so a new cell can't silently skip). The rstest per-case granularity
  collapses to one loop-per-file with per-cell `catch_unwind` skip/fail
  classification (a missing fixture skips that cell, never aborts the rest). A
  coordinate that maps to MORE than one row (realtime's `(Native,Cpp)` →
  component + #124 rclcpp) is preserved via `exec_for -> Vec<Exec>`, so the
  strict derive drops NO sub-variant coverage.
- [x] Gate G5 — **landed** as `matrix::tests::g5_w1_consumers_claim_their_owned_workloads`
  (in `matrix.rs`, not `matrix_fixture_coverage.rs` — it gates the
  `w1_consumer_of` PARTITION, which lives there). Asserts every Runtime cell of
  a fully-owned workload (`Multihost`/`RealtimeTiers`/workspace `Service`|`Action`)
  is claimed, and every consumer claims ≥1 cell. Exec-arm totality is
  runtime-enforced (the hard panic above), since exec tables live in the test
  binaries. `EntryPubsub` + native feature workloads are SHARED with W4 files, so
  only the claimed subset is gated (full ownership follows once W4 converts).
- [ ] Kill the second spelling: `platform_header_matrix.rs` local `CELLS`
  table rewritten over `matrix::PlatformId` (its cells move to the
  compile-check fixture family in W5) — **deferred to W5** (its own text ties it
  to the compile-check fixture move).

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

### W8 — cut the BUILD, not just the file count (added 2026-08-04; row-dedup half RETRACTED same day)

W0–W7 consolidate the *consumer* side (fewer files, one table per bucket) but
barely move build wall-clock — the same fixtures still build. This wave was meant
to attack the fixture-BUILD burden directly by deleting redundant `fixtures.toml`
rows / shrinking the edition sweep. **On verification (2026-08-04) every row-dedup
candidate proved load-bearing** — see the per-item RETRACTED notes and the W8
verdict at the end of this section. The one surviving lever is **W8.d**, and it
DEPENDS ON W1. Net: there is no manifest-only build cut; build relief comes from
the consumer-side waves (fewer boots, fewer files) plus W8.d after W1.

- [x] ~~**W8.a — drop the `target-zenoh` twins.**~~ **RETRACTED 2026-08-04 —
  the premise is wrong.** The `target-<rmw>/` dirs are NOT redundant duplicates;
  they are the RUNTIME fixture locations the binary-locator requires:
  `fixtures/binaries/mod.rs:718-724` maps `Rmw::Zenoh → "target-zenoh"` (and
  xrce/cyclone likewise), so a runtime cell with `rmw=Zenoh` resolves its binary
  under `target-zenoh/`. Deleting the twin breaks binary resolution. The only
  place a bare (no-`rmw`, default `target/`) row co-exists with a `target-zenoh`
  twin is NATIVE rust, and they are NOT mutually redundant: the bare row is a
  compile-ASSERT fixture (`fixtures.toml:1060` "build-assert them"; native,
  cheap, skipped by coord-filtered lanes since it has no `rmw` — `fixtures-manifest.py:227`),
  the twin is the runtime fixture. On the EXPENSIVE platforms (threadx-linux,
  threadx-riscv64) every rust row is a single `rmw=zenoh` runtime fixture in
  `target-zenoh` — there is NO twin to drop. `--core-only` (#29) is an
  incremental REBUILD-skip, not proof of deletability. Net: no safe build cut
  here; the agent-2 R1 candidate does not survive verification.
- [x] ~~**W8.b — the expensive build-only twins go first.**~~ **RETRACTED —
  same root cause.** threadx-riscv64/qemu action rows carry no bare sibling; each
  `target-zenoh` row is the sole runtime fixture (locator-addressed). Nothing to
  drop.
- [x] ~~**W8.c — shrink the edition sweep to its unique signal.**~~ **RETRACTED
  2026-08-04 — does not survive inspection of the assertions.** `ros_editions_e2e.rs:160`
  asserts real cross-wire DELIVERY per `(rmw × workload × dir)`, and each workload
  carries a DIFFERENT message type (`std_msgs/Int32` pubsub vs the service type vs
  the action types) whose EDITION-SPECIFIC RIHS01 hash is exactly the interop risk
  the edition axis exists to catch (#0291). Collapsing workloads drops type
  coverage; the two directions exercise nano-as-publisher vs nano-as-subscriber
  hash matching — not redundant either. The "only unique signal is the per-edition
  hash tail" premise is false: the tail differs PER TYPE, so per-workload is load-bearing.
- [ ] **W8.d — make `ci-matrix` build what its gate scopes. DEPENDS ON W1 —
  not independent (corrected 2026-08-04).** Tier 2 builds `all` and runs full
  `test-all`; only the staleness gate is coordinate-scoped, so the 26% is
  gate-only. But the `justfile:1904-1909` comment (issue 0393) records why the
  build is deliberately `all`: narrowing the build requires narrowing the RUN
  (`NROS_TEST_SCOPE`) to the same coordinates FIRST, else tests execute with no
  fixture. Today `NROS_TEST_SCOPE` is `native`-granularity only; a per-coordinate
  run-scope needs each test to know its coordinate and skip when outside the lane
  — which is exactly what W1 (tests derive from `matrix::CELLS`) provides. So W8.d
  lands AFTER W1, and adds: a `NROS_TEST_SCOPE=coords:<file>` the cell-bound tests
  honour, then narrow build+run+gate to one coords file. Not the standalone recipe
  edit the first draft assumed.
- [x] ~~**W8.e — collapse same-code/different-YAML rows.**~~ **RETRACTED — not
  same-code.** robot1/robot2 (`fixtures.toml:225/505/708`) share dir/bringup but
  differ by `entry`: robot1 bakes the TALKER, robot2 the LISTENER (comment
  `:500`); `multihost_e2e` spawns BOTH as two cross-host processes. They are the
  two halves of one multihost pair — same class as the #0096-blocked server/client
  doubling, both needed. No merge.

- **W8 verdict (2026-08-04): every row-dedup item failed verification — the
  fixtures the 4-agent sweep flagged as redundant are load-bearing.** `target-<rmw>/`
  is the runtime binary locator (`binaries/mod.rs:718`), edition workloads carry
  distinct per-type edition hashes, robot1/2 are talker/listener halves. The
  fixture-BUILD burden is NOT reducible by deleting rows — it is structural (each
  row is a genuine platform×lang×rmw×workload artifact). The real build relief is:
  **(1) W8.d after W1** — narrow the tier-2 RUN to its coordinates so the BUILD can
  narrow too (the one surviving W8 lever, and it needs the consumer loop first);
  **(2) fewer redundant BOOTS** via W2/W4 (a 10-file family booting one image each
  → one rstest); **(3) fewer FILES** to compile+link at test time via W5. Chase the
  consumer side (W0→W1→W2…), not the manifest.

## Validated inventory (4-agent full-tree sweep, 2026-08-04)

The 2026-08-01 numbers in "Problem" above were re-measured against the current
tree. Refinements, so execution targets the real sites:

- **matrix `CELLS` = 202** (174 Runtime / 17 BuildOnly / 11 CarveOut), `matrix.rs:388`.
  **`interop::CELLS` = 11** (10 Runtime), `interop.rs:212`. Confirmed only 3 files
  iterate a CELLS list (`alloc`, `ci_lane`, `zephyr.rs` count-only).
- **Fixtures = 344 build rows** (251 `[[fixture]]` + 93 `[[workspace_fixture]]`) +
  26 `[[compile_check_fixture]]`. Native/cheap = 187 rows; the rest are
  QEMU/cross. This is the W8 cost surface the original phase text did not quantify.
- **Sched-dim family = 10 files** (W2), each a per-(platform,dim) copy of one
  boot-and-classify test; the shared form already exists for one dim
  (`zephyr_edf_deadline_applied.rs:35 assert_edf_applied`) — generalize it.
- **Bridge family = 4 files** (W3/W4), a copy-pasted 2×2 (imperative|declarative ×
  cyclone|xrce); `bridge_zenoh_to_cyclonedds.rs:1` docstring names itself a
  sibling copy. Fold using the existing `ros_env.rs` bridge helpers.
- **8 hand-written zephyr↔native interop fns** (`zephyr.rs:806–1574`) sit beside
  the `example_e2e(#[case])` consumer they should be rows of (W4).
- **Exhaustive-match landmines** the file-fold must also remove:
  `zephyr.rs:119-130/540-544/574` `unreachable!` arms panic on a new Rmw/Lang/
  Workload; `PlatformId` needs edits in 4 places per new platform (`matrix.rs:60-161`).

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
- **Build cost drops via the consumer side, not manifest dedup (W8 verdict):**
  the row-dedup candidates proved load-bearing; relief is fewer redundant image
  boots (W2/W4) and — after W1 — a coordinate-scoped tier-2 RUN letting `ci-matrix`
  build only its lane coords (W8.d).

## Sequencing note

W1 and W2 touch `realtime_tiers_e2e` — land after the issue-0380 model
restoration settles (this phase's W2 gate is 0380's structural fix).
W4 is the long tail; it can proceed cell-family by cell-family behind the
W1 gates without a flag day.

**W8's row-dedup items (a/b/c/e) are RETRACTED** (verified load-bearing,
2026-08-04) — do not attempt them. **W8.d** is the only survivor and lands
AFTER W1 (it needs coordinate-scoped test runs, which the cell-bound consumers
provide). Recommended order therefore starts on the consumer side: **W0 → W1**
(close the consumer loop + gate G5) → **W8.d** (narrow the tier-2 run+build) →
**W2** (sched-dim 10→1) → W3/W4/W5/W6/W7.
