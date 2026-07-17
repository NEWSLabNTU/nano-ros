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

> As-landed (2026-07-17): EVERY baked port/domain now comes from
> `nros_tests::alloc` (`port_of` / `xrce_agent_port_of` / `domain_of` —
> `7000 + platform*400 + workload_offset + lang*100`; agents 2000+;
> domains `1 + platform*21 + slot*3 + lang`). `legacy_port` deleted;
> `platform.rs` bases are now DERIVED (`alloc::platform_port_base`) with
> `lang_stride = 100` everywhere. Window map: zephyr 7400/2400/dom 22–30,
> freertos 7800, nuttx-arm 8200, nuttx-riscv 8600, threadx-linux 9000
> (cyclone dom 107/108, ex 61), threadx-riscv64 9400 (dom 127/128/129, ex
> 62), esp32 9800, baremetal 10200, fvp 11000+. Re-baked sides:
> fixtures.toml (74 locator/domain values), per-example Cargo deploy
> metadata (~40 crates incl. baremetal/esp32), zephyr-fixture-leaves.sh
> (example formula + 12 ws-entry literals), just/threadx-*.just domains,
> large-msg firmware; consumers (entry_e2e / realtime_tiers_e2e /
> multihost_e2e / c_riscv_nuttx_e2e / qos_zephyr_ros2_interop_e2e /
> freertos_run_plan_runtime / emulator / large_msg / esp32_emulator /
> native_api) now CALL the allocator instead of mirroring literals. This
> killed the 17851 overlap (safety-zephyr 7490 vs freertos-cpp tiers
> 8091) and the whole hand-mirrored 175xx band. The matrix gained the
> 5 RealtimeTiers Runtime cells the consumer always ran but the seed
> table never modeled (nuttx-arm c/rust, nuttx-riscv rust/c, freertos c).
> A `#[test]` emitter (`alloc::tests::print_bake_table`) prints the full
> bake table for future re-bakes. Multi-image demo sets that outnumber
> the workload axis (baremetal BSP / RTIC-mixed / large-msg) take named
> `alloc::aux_port` slots 10500/10510/10520, asserted collision-free by
> the injectivity test.

- [x] W4.a Re-bake fixtures.toml locators/domains from the allocator —
  zephyr cyclone 50–58 → 22–30, threadx 61/62 → 107/108 + 127/128/129,
  the 175xx band and every 745x/75xx/76xx base onto the formula (see the
  as-landed note).
- [x] W4.b baremetal + esp32 off `lang_stride = 0` (both now 100; rtic
  pair 10200/10210/10220, bsp/mixed/large-msg aux 10500/10510/10520;
  esp32 pubsub/service/action 9800/9810/9820, ws-entry 9830). The
  `qemu-baremetal-shared` group + its override are retired; the blanket
  serial `qemu-esp32` group is retired — only the three tests dialing the
  SAME baked pubsub-pair image keep a serial `qemu-esp32-pubsub-port`
  group (one image, three tests — a genuinely exclusive resource; the
  boot smoke + ws-entry e2e run free; `just test`'s fast-path filter now
  excludes esp32 by `binary(esp32_emulator)`).
- [x] W4.c nextest groups reduced: the 12 per-variant rtos_e2e overrides
  + the 12 per-variant sub-groups (`qemu-{freertos,nuttx,threadx-riscv}-
  {pubsub,service,action}`, `threadx-linux-*`) are gone. Survivors each
  name their resource: `zephyr-fvp` (node-locked FVP license + fixed
  UART telnet ports), `ros2-interop` (singleton ros2 daemon),
  `host-dds-ros2-interop` (shared 232-slot DDS domain space),
  `zephyr-qos-port` + `qemu-esp32-pubsub-port` + `qemu-freertos-entry`
  (one baked image serving several tests), and the per-platform
  `qemu-*`/`threadx-linux` groups (QEMU/host-load throttles only).
- [x] W4.d Fresh rebuild of native / threadx-linux / freertos / nuttx /
  threadx-riscv64 / zephyr (+ baremetal, esp32) fixture families on the
  new bakes + consumer sweep — results below. No `just test-all`
  before/after pair was captured: the pre-W4 tree's fixtures were already
  stale against W1–W3 (a "before" number would have been a museum-binary
  measurement — the 0148 lesson), so the wall-clock recorded here is the
  after-only baseline for the next phase to compare against.
  <!-- W4.d-results -->


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
- [x] W7.a (landed 2026-07-17 with the RFC, commit b75b8c028) Checklist §E gains: **E6** every runtime lane derives from the
  matrix SSoT (no hand-written cell files; detect: new `tests/*_e2e.rs`
  not consuming `matrix::CELLS`); **E7** output-marker discipline (all
  literals live in `output.rs`; the W2.b gate is the detector); **E8**
  isolation discipline (no port/domain literals outside `alloc` +
  fixtures.toml derived columns; detect: `grep -rnE ':(7[0-9]{3}|17[0-9]{3})' tests/`);
  **E9** launch-convention conformance (runner metadata, not hand-rolled
  emulator lines outside `qemu.rs`) + micro-test budget (phase-named test
  files = automatic finding; tests/ file-count trend recorded per audit).
- [x] W7.b (same commit — testing lane widened to E1–E9) `.claude/skills/audit/SKILL.md`: test-system audit named as a
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
