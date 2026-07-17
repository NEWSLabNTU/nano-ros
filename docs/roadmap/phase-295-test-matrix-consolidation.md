# Phase 295 — test-matrix consolidation (RFC-0051)

Status: **Draft — 2026-07-17** · Implements
[RFC-0051](../design/0051-test-matrix-architecture.md) · Touches the whole
`packages/testing/nros-tests` surface, `examples/fixtures.toml`,
`.config/nextest.toml`, `docs/development/codebase-audit-checklist.md`.

> **Goal.** One declared matrix (platform × language × RMW × workload ×
> {example, workspace, interop}) generates the fixture rows, the test
> lanes, and the port/domain assignments; one standard-node checker
> replaces ~55 inline copies; parallelism comes from allocation, not
> nextest serialization; launch lines come from framework runner metadata;
> the phase-probe micro-tests die. Survey basis: the two 2026-07-17
> inventory reports (193 test files; 245 + 82 fixture rows; 22 hardcoded
> `"Received:"`; 27 files hand-agreeing 175xx ports; 4 isolation schemes).

## Waves

### W1 — matrix + allocator core (no lane behavior change)
- [x] W1.a `nros_tests::matrix` — the Cell table (RFC-0051 §1) seeded from
  today's REAL coverage: every existing runtime lane becomes a Runtime
  cell; known gaps become BuildOnly/CarveOut with the reason string.
  Grow `rtos_e2e`'s Platform/Lang enums; do not fork them.
- [x] W1.b `nros_tests::alloc` — port/domain formula + build-stage
  injectivity assertion; native stays ephemeral. Document the 7000+ band.
- [x] W1.c `matrix-gen` (build-stage bin): verifies fixtures.toml rows
  against the matrix + allocator (derived locator/domain columns match);
  `examples_fixture_coverage.rs` re-pointed at it (⊇ both directions).

### W2 — one checker
- [x] W2.a `nros_tests::checker::StandardChecker` wrapping
  ready-wait → collect → count → monotonic-assert for every workload,
  built on the existing `output.rs` constants/parsers.
- [x] W2.b Migrate the matrix consumers (W3) onto it; add the
  literal-marker gate (markers outside `output.rs` fail a grep test).
  Kill the 22 `"Received:"` literals.

### W3 — consolidate the per-cell files into matrix consumers

> As-landed (2026-07-18, commits d9a69c570 / 9deb721dd / 987593246 /
> 6134a8286 / fb3dd5956): tests/ 193 → 127 files. New consumers:
> realtime_tiers_e2e (16 cells), roundtrip_xprocess_e2e (8),
> multihost_e2e (5), workspace_features_e2e (16), entry_e2e (15), and
> zephyr.rs's in-file example_e2e (27 cells; 74 fns → 20). Bonus finds:
> six files had `let _ = router;` DROPPING the zenohd guard immediately
> (latent since phase-263 — fixed); three families' "baked ports" were
> fiction (now ephemeral); several seed-table lies corrected in both
> directions by the W1 gates; 17851 port overlap (safety-zephyr vs
> freertos-cpp tiers) flagged for W4. The reverse fixtures⊆matrix gate
> is now an ASSERT.
- [x] W3.a `rtos_e2e` absorbs: `freertos_qemu.rs`, `nuttx_qemu.rs`,
  `threadx_riscv64_qemu.rs` smoke overlap; threadx-riscv64 action cell
  added (or CarveOut'd with reason).
- [x] W3.b `entry_e2e` matrix file absorbs the ~20 `*_entry_e2e` files;
  `workspace_e2e` matrix file absorbs the ~12 `*_workspace_e2e` +
  `*_roundtrip_xprocess_e2e` ×8; `realtime_tiers_e2e` absorbs the ×15
  family; `multihost_e2e` absorbs the ×6. Per-file deletion only AFTER the
  matrix cell is green on fresh fixtures (the #215 rename lesson: grep for
  binary-name/test-name consumers when retiring files).
- [x] W3.c zephyr.rs (74 fns): parametrize the pubsub/service/action/entry
  families into cells; keep the genuinely bespoke ones (xrce serial,
  tx-throughput measurement is retired per W5).
- [x] W3.d Retire/promote phase-probe one-offs: `w1d_*`, `w1_zephyr_tx_*`,
  `phase_118_collapse`, `native_entry_poc_boot`, CLI `phase_212_f_*`,
  `phase215_f_*` (E3 names). Behavior worth keeping gets a matrix cell or
  a behavior-named gate; the rest are deleted with a line in this doc.

### W4 — isolation migration (parallelism unlock)
- [ ] W4.a Re-bake fixtures.toml locators/domains from the allocator
  (mechanical; matrix-gen emits the diff). Includes the zephyr cyclone
  50–58 band, threadx 61/62, the 175xx band — all onto the formula.
- [ ] W4.b Migrate baremetal + esp32 off `lang_stride = 0`; retire the
  `qemu-baremetal-shared` / `qemu-esp32` serialization groups.
- [ ] W4.c nextest.toml groups reduced to genuinely exclusive resources
  (fvp license, ros2 daemon, host-load throttles); document each survivor
  with the resource it guards.
- [ ] W4.d Full-parallel sweep proof: `just test-all` wall-clock before /
  after recorded here.

### W5 — launch via framework runner metadata
- [ ] W5.a Zephyr: interpret `runners.yaml` from the prebuilt build dir
  (west-fvp-run pattern generalized); native_sim keeps direct exec (it IS
  the framework convention there).
- [ ] W5.b ESP-IDF: derive the QEMU/espflash line from
  `flasher_args.json`.
- [ ] W5.c NuttX/FreeRTOS/baremetal: per-machine QEMU argument blocks move
  next to the board crates (phase-215 duty rule); `qemu.rs` becomes the
  interpreter. Sanctioned-bypass doc-comments at any callsite that can't
  use the framework runner (E1-exception pattern).

### W6 — RMW runtime-coverage triage (the matrix makes debt visible)
- [ ] W6.a Per-cell Runtime-vs-CarveOut decision for cyclonedds + xrce on
  each RTOS platform (cyclone needs POSIX+CPP Kconfig; xrce needs agent
  bake); implement the Runtime ones, record the CarveOuts in the table.
- [ ] W6.b Workspace RTOS cells same triage (today 62/82 rows native).
- [ ] W6.c Interop cells: one reduced workload set (pubsub + service)
  × {rmw_zenoh, rmw_cyclonedds, rmw_fastrtps, xrce} against real ROS 2,
  reusing StandardChecker on the ROS-side output.

### W7 — audit-skill extension
- [ ] W7.a Checklist §E gains: **E6** every runtime lane derives from the
  matrix SSoT (no hand-written cell files; detect: new `tests/*_e2e.rs`
  not consuming `matrix::CELLS`); **E7** output-marker discipline (all
  literals live in `output.rs`; the W2.b gate is the detector); **E8**
  isolation discipline (no port/domain literals outside `alloc` +
  fixtures.toml derived columns; detect: `grep -rnE ':(7[0-9]{3}|17[0-9]{3})' tests/`);
  **E9** launch-convention conformance (runner metadata, not hand-rolled
  emulator lines outside `qemu.rs`) + micro-test budget (phase-named test
  files = automatic finding; tests/ file-count trend recorded per audit).
- [ ] W7.b `.claude/skills/audit/SKILL.md`: test-system audit named as a
  first-class category pass (E, with the E6–E9 additions), so `/audit
  quick E` runs the test-system sweep standalone.

## Non-goals
- Touching the ~660 in-crate unit tests (they test code, not matrix cells).
- Deleting drift/shape gates (`example_shape`, `*_drift`, `loc_budgets` —
  cheap and load-bearing).
- Changing example NODE behavior (the checker pins the stock-ROS-demo
  contract the nodes already follow).

## Acceptance
- Every runtime lane is a matrix cell; `matrix-gen` verifies fixtures.toml
  ⊆⊇ matrix; injectivity of (port, domain) asserted.
- Zero output-marker literals outside `output.rs`; zero port literals
  outside `alloc`/fixtures.toml derived columns (both gated).
- tests/ file count reduced ~40 with cell coverage strictly increased;
  every carve-out has a reason string in the table.
- `just test-all` parallel wall-clock improvement recorded.
- Audit checklist E6–E9 live; `/audit quick E` exercises them.
