# Phase 295 — test-matrix consolidation (RFC-0051)

Status: **Complete — 2026-07-18** (Draft 2026-07-17) · Implements
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

> As-landed (2026-07-17, this commit): the launch plumbing now READS the
> framework's build-stage runner metadata instead of hand-coding the line
> where such metadata exists. New interpreters (both tiny hand-rolled
> readers, no new deps — same flat-parse style as `fvp.py`'s CMakeCache
> read): `zephyr::RunnersYaml` (parses `<build_dir>/zephyr/runners.yaml` —
> `flash-runner`, `runners:` list, `config.exe_file`/`elf_file`) and
> `esp32::EspFlasherArgs` (parses `<build_dir>/flasher_args.json` via
> serde_json — `extra_esptool_args.chip` → QEMU `-M`, `flash_settings`).
> Validation (fixtures fresh from W4, no rebuild): native_sim zephyr rust
> pubsub e2e via the new `runners.yaml` path PASS; zephyr boot_smoke PASS;
> freertos rust pubsub qemu lane PASS (launch unchanged); esp32 boot lane
> PASS with the derived `-M esp32c3`; matrix_fixture_coverage +
> output_marker_gate + all 52 lib unit tests green. No lane behavior
> changed — this is launch-source plumbing only.

- [x] W5.a Zephyr: `ZephyrProcess::start` (native_sim) now derives its launch
  binary from `runners.yaml` — confirms `flash-runner: native` and runs
  `config.exe_file`, the SANCTIONED framework form (the `native` runner just
  host-execs `zephyr.exe`; documented at the callsite as NOT an E1/E9 bypass);
  falls back to the passed binary when the file is absent (identical effect).
  The dormant `QemuArm` branch reads `runners.yaml` for the `qemu` runner +
  `elf_file`; its `-cpu/-machine` block stays hand-rolled under a
  sanctioned-bypass doc-comment (Zephyr keeps QEMU flags in `board.cmake` →
  the CMake `run` target, reachable only via `west build -t run` which trips
  E1). `west build -t run` NOT used.
- [x] W5.b ESP-IDF: `esp32::start_esp32_qemu{,_mcast}` derive the QEMU `-M`
  machine from a sibling `flasher_args.json` when present (ESP-IDF `idf.py`
  layout). The esp32 Rust examples flash an `espflash save-image --merge`
  blob with NO `flasher_args.json`, so that path falls back to `esp32c3`
  under a documented sanctioned exception (no framework metadata exists for
  the merged-image path). `create_esp32_flash_image` (the espflash line)
  stays as-is — it is build-stage image prep, not the launch line.
- [x] W5.c NuttX/FreeRTOS/baremetal: no framework runner exists, so the
  hand-rolled `qemu.rs` builders ARE the convention and are KEPT. Chose the
  doc-comment route (prompt-preferred, avoids disturbing the many green QEMU
  lanes): a module-level SANCTIONED-BYPASS doc-block in `qemu.rs` explains the
  E1/E9 exception for all `start_*` builders. FOLLOW-UP (deferred): relocate
  each per-machine `-M/-cpu/-netdev` block into the owning board crate's
  `NROS_BOARD_RUNNER`-adjacent metadata (phase-215 duty rule) so `qemu.rs`
  becomes a thin reader of board-provided launch metadata.

### W6 — RMW runtime-coverage triage (the matrix makes debt visible)
- [x] W6.a (2026-07-18) every cyclone/xrce-on-RTOS gap cell decided: implement-worthy → issue #233, rest → firm CarveOut reasons in matrix.rs (zero "W6 decides" placeholders left) Per-cell Runtime-vs-CarveOut decision for cyclonedds + xrce on
  each RTOS platform (cyclone needs POSIX+CPP Kconfig; xrce needs agent
  bake); implement the Runtime ones, record the CarveOuts in the table.
- [x] W6.b workspace RTOS RMW cells modeled + reasoned in the table (thin-zenoh debt folded into #233) Workspace RTOS cells same triage (today 62/82 rows native).
- [x] W6.c Interop cells: one reduced workload set (pubsub + service)
  × {rmw_zenoh, rmw_cyclonedds, rmw_fastrtps, xrce} against real ROS 2,
  reusing StandardChecker on the ROS-side output.

> As-landed (2026-07-17): the ROS 2 interop family → ONE matrix consumer
> `tests/interop_e2e.rs` (9 rstest cases over the `Kind::Interop` cells).
> `git rm`'d: `rmw_interop.rs` (31 fns), `cyclonedds_ros2_interop.rs`,
> `demo_nodes_cpp_interop.rs`, `ros2_lifecycle_interop.rs`. Cases: zenoh
> pubsub (nano→ros2, ros2→nano, + stock `demo_nodes_cpp` cross-vendor),
> zenoh service (nano-server, ros2-server), cyclone pubsub (both dir),
> cyclone service (nano-server), zenoh lifecycle full-cycle — mapping 1:1
> onto the reduced Interop matrix cells (Native·Rust·{Zenoh,Cyclonedds}·
> {Pubsub,Service} + Native·Rust·Zenoh·Lifecycle). `checker::assert_delivery`
> asserts every nano-ros / `demo_nodes_cpp` endpoint (RFC-0051 §2); the raw
> `ros2 topic echo`/`service call` DDS/CLI sinks count wire fields
> (`data:`/`sum`, not nano demo markers → gate-clean). Skip semantics
> preserved verbatim (require_ros2 / require_ros2_cyclonedds + fixture/peer
> skips). **Matrix: unchanged** — the reduced Interop set already models
> exactly these lanes; no cells added. No `rmw_fastrtps` interop lane exists
> to fold (fastrtps is only the DDS peer *inside* the bespoke XRCE test).
>
> **Kept bespoke** (own binaries, not folded): `xrce_ros2_interop.rs` (XRCE
> Agent lifecycle specifics; covers the Xrce Pubsub/Service Interop cells)
> and `qos_zephyr_ros2_interop_e2e.rs` (on-target zephyr QoS interop,
> `zephyr-qos-port` group; the ZephyrNativeSim·Cpp·Cyclonedds·Qos cell).
>
> **Retired in the reduction** (were `rmw_interop.rs`-only, outside the
> reduced pubsub+service+lifecycle delivery set; each pinned via the
> nano↔nano example cells or is perf/introspection, not a matrix cell):
> the 3 detection probes + `keyexpr_format` (no-assert env reports); the 5
> discovery-visibility lanes (`ros2 node/topic/service list` liveliness);
> the QoS RxO lanes (`qos_compatibility` + `qos_matrix` ×4); the 3 action
> interop lanes (nano↔stock rclcpp_action — the action WIRE protocol stays
> covered by the Zenoh/Cyclone `Action, Example` cells); the 4 rate/latency/
> throughput benchmarks (perf, per the W5 tx-throughput-measurement retire).
> Restoring any of these = re-adding a matrix cell + a consumer case.
>
> Nextest routing: `binary(interop_e2e) and test(cyclone)` →
> `host-dds-ros2-interop` (232-slot DDS domain space); `binary(interop_e2e)`
> → `ros2-interop` (serial singleton ros2 daemon). The three retired
> `binary(rmw_interop|cyclonedds_ros2_interop|ros2_lifecycle_interop)`
> overrides collapsed into these two. `just native test-ros2` /
> `test-ros2-lifecycle` re-pointed at `interop_e2e`; `Cargo.toml` `[[test]]`
> renamed. Validation: check/clippy `--all-targets` clean; nightly fmt;
> output_marker_gate + matrix_fixture_coverage + 52 lib tests green; ALL 9
> cells PASS against live ROS 2 humble + rmw_zenoh_cpp + rmw_cyclonedds_cpp
> (cyclone fresh; the zenoh/lifecycle native-rust fixtures were mtime-stale
> from a stale in-tree CLI — the same treadmill the old tests hit — so they
> were exercised with `NROS_SKIP_FIXTURE_CHECK=1`, all delivering).

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

## Outcome (2026-07-18)

All waves landed. `packages/testing/nros-tests/tests/` 193 → ~120 files;
every runtime lane is now a cell of `nros_tests::matrix` consumed by a
parametrized file (example_e2e in rtos_e2e/zephyr, entry_e2e,
workspace_features_e2e, realtime_tiers_e2e, roundtrip_xprocess_e2e,
multihost_e2e, interop_e2e); one `checker::assert_delivery` + `output.rs`
markers (marker-literal gate live); one injective `alloc` port/domain
formula (fixtures + tests derive from it — the 175xx hand-mirroring and the
17851 overlap gone; qemu-baremetal-shared + qemu-esp32 serialization groups
retired); launches read framework runner metadata (zephyr runners.yaml, IDF
flasher_args.json); the fixtures⊆⊇matrix + injectivity + marker gates keep
it honest. RMW-runtime debt is visible as reasoned BuildOnly/CarveOut cells
(#233 tracks the implement-worthy ones). Audit §E6–E9 + the skill's E1–E9
testing lane landed with the RFC.
