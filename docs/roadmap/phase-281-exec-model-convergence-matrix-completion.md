# Phase 281 — Complete the Model-1 execution-model convergence (all language × platform)

Status: **W3 complete (2026-07-07)** — W1 ✓, W2 ✓, W3 ✓ (Zephyr C/C++ tiers proven on
native_sim), W5 ✓ landed; only the nuttx cells remain (W4), gated on phase-280 ·
Implements the RFC-0015 Model 1 convergence to
its full lang×platform matrix · Finishes what phases 272–274 started · Cross-links
issues #128 (Zephyr macro parity lineage) and #130 (nuttx runtime, owned by
phase-280) · Informs phase-263 (broad feature-workspace completion) and phase-275
(#102 fixture coverage).

> **Goal.** Phase-274 decided **Model 1** (one executor per priority tier over one
> shared session, `active_groups`-gated) as the single execution model for ALL
> languages, and proved it on native (Rust/C/C++) and FreeRTOS (C++). This phase
> closes the remaining cells of the language × platform matrix: every cell is
> either **proven by an end-to-end tier test** or **explicitly deferred with a
> reason** — no silent gaps ("no silent caps"). The deliverable is a canonical
> multi-tier example + e2e per open cell, plus a matrix gate that fails when a cell
> regresses to unproven.

## Baseline — the convergence matrix (2026-07-07)

Verified this session where marked ✓. Cells are the `run_tiers` / Model-1 tier path,
proven by the named end-to-end test.

| lang \ platform | native | freertos | zephyr | nuttx |
| --- | --- | --- | --- | --- |
| **Rust** | ✓ `realtime_tiers_e2e` | ✓ `orchestration_tiers_freertos` (both tests green after W1) | ✓ `realtime_tiers_zephyr_entry` | gated → phase-280 |
| **C++**  | ✓ `realtime_tiers_cpp` (+rclcpp, +subnode) | ✓ `realtime_tiers_cpp_freertos` (3-tier, #144) | ✓ `realtime_tiers_cpp_zephyr_e2e` (W3b) | gated → phase-280 |
| **C**    | ✓ `realtime_tiers_c_e2e` | ✓ `realtime_tiers_c_freertos_e2e` (W2, landed) | ✓ `realtime_tiers_c_zephyr_e2e` (W3c) | gated → phase-280 |

Legend: ✓ proven e2e · GAP = no tier path/example/test built · gated = model runs,
runtime plumbing open (tracked elsewhere).

Notes on the non-obvious cells:
- **C × freertos**: the `nros_board_freertos_run_tiers` C implementation is shared and
  is exercised by the C++ freertos e2e, but **no C-*node* multi-tier example or test
  exists** — the cell is unproven for a C node. (W2.)
- **Rust × freertos**: the boot+`run_tiers` test (`multi_tier_freertos_firmware_builds_
  and_boots_run_tiers`) is **green** — run_tiers executes on device. The stronger
  connected-cross-process test (`…_connects_over_slirp_and_runs_tiers`) is NOT reliably
  green: it calls `firmware_release()` which does a **`cargo build --release` at run
  time** (orchestration_tiers_freertos.rs — an anti-pattern that violates "no compilation
  inside tests"), so it is slow + fragile and times out at 60 s. (Two source facts also
  surfaced: a stale `build/compile-check/` copy predating the `platform-freertos` feature
  move `nros` → `nros-platform` gave a false *build* failure until cleaned; the source
  fixture is correct.) W1 moves this test onto a build-stage Release fixture like the C++
  path.
- **nuttx** (all langs): the entry-link convention landed (#127) but runtime networking
  (eth0 IP push) is still being wired — owned by **phase-280**. This phase depends on it,
  does not duplicate it.

## Why

The matrix visualization (unified-execution-model artifact, 2026-07-07) exposed that
"Model 1 for all languages" is a decided model but an *incompletely proven* one: 3 of
12 cells are real build/test gaps and 3 are gated on runtime plumbing. A user picking
`C` + `Zephyr`, or `C` + `FreeRTOS`, has no working tiers example. RFC-0015's headline —
one tier plan deploys unchanged anywhere — is not yet true end to end.

## Waves

### W1 — Harden the proven-but-fragile cell (Rust × freertos)
The boot+`run_tiers` test is already green; this wave makes the connected cross-process
delivery assertable within budget.
- [x] W1.a Move `orchestration_tiers_freertos`'s connected test off its run-time
  `cargo build --release` (`firmware_release()`) onto a **build-stage Release fixture**
  (mirror `realtime_tiers_cpp_freertos_e2e` + the `CMAKE_BUILD_TYPE=Release` /
  `--release` compile-check row), so a -O0 zenoh-pico no longer starves the handshake and
  the test asserts `[ctrl]`/`[telem]`-style per-tier delivery in ≤ the nextest budget.
  This also removes a "no compilation inside tests" violation.
- [x] W1.b Guard the stale-copy recurrence: the pre-fix `build/compile-check/` copy
  (with `platform-freertos` on `nros`) gave a false *build* failure until cleaned — the
  same staleness class phase-278 (#147) addressed at run stage. Ensure the compile-check
  builder rebuilds on source change / the resolver flags a stale `.compile-ok`; file a
  follow-up if the compile-check lane isn't covered by #147's dep-info probe.

### W2 — C-node multi-tier on FreeRTOS (close C × freertos)
- [x] W2.a Add `examples/workspaces/ws-realtime-c-mps2` — the C sibling of
  `ws-realtime-cpp-mps2`: a C `ctrl` node + C `telem` node on two tiers over one shared
  session via `nano_ros_entry(BOARD mps2-an385-freertos …)`, each printing `[<tier>] tick=`
  on a successful publish.
- [x] W2.b Fixture row + `realtime_tiers_c_freertos_e2e` asserting both tiers publish
  cross-process under QEMU (mirror `realtime_tiers_cpp_freertos_e2e`). This proves the
  shared `nros_board_freertos_run_tiers` C impl drives a C *node*, not only a C++ one.

### W3 — Zephyr C/C++ tiers (close C×zephyr + C++×zephyr)
- [x] W3.a Macro/codegen parity: the Zephyr C/C++ entry emit currently wires only
  register+spin (the #128 lineage — the Rust Zephyr arm has the tier path, C/C++ do not).
  Extend the Zephyr C/C++ entry codegen to emit the `run_tiers` shape (needs a
  `ZephyrBoard` C/C++ `run_tiers` seam, mirroring the FreeRTOS C `nros_board_freertos_run_tiers`).
  Landed as the W3a `ZephyrBoard::run_tiers` seam + codegen (commit e65bc58bb).
- [x] W3.b Add a Zephyr C++ realtime-tiers example
  (`examples/workspaces/ws-realtime-cpp/src/zephyr_entry`) + `realtime_tiers_cpp_zephyr_e2e`
  (native_sim, both tiers deliver). First full west link + runtime proof of the W3a seam
  (cpp/zephyr COVERED).
- [x] W3.c Add the Zephyr C sibling
  (`examples/workspaces/ws-realtime-c/src/zephyr_entry`, `[tiers.*.zephyr]` priorities +
  `[deploy.zephyr]`, `wscrt_*` west lane, `build_zephyr_workspace_c_realtime_entry()`) +
  `realtime_tiers_c_zephyr_e2e` (native_sim, both tiers deliver). Closes c/zephyr — the
  ZephyrBoard::run_tiers seam is now proven for a C node too. W3 complete; only the nuttx
  cells remain (W4, gated on phase-280).

### W4 — nuttx cells (dependency, not duplication)
- [ ] W4.a Track phase-280 (nuttx entry eth0 + runtime proof). When it lands, add the
  nuttx tier cells (Rust + C) to the matrix gate. This phase does NOT re-implement the
  eth0 plumbing.

### W5 — Matrix gate (no silent caps)
- [x] W5.a A test or check that enumerates the lang×platform tier matrix and asserts each
  cell is either (a) covered by a named e2e or (b) listed in an explicit deferral set with a
  reason — so a regression that drops a cell to unproven fails CI, and the matrix in this
  doc / the artifact stays honest. Mirror the `examples_fixture_coverage.rs` "no silent
  caps" pattern.

## Out of scope
- The broad feature-workspace completion (services/actions/params/lifecycle/QoS/bridges in
  all four languages) — that is **phase-263**. This phase is scoped to the *execution-model
  tier* cell of that larger grid.
- nuttx runtime networking — **phase-280**.
- Zephyr tx-throughput ceiling — **phase-279**.

## References
- RFC-0015 (Model 1 execution model, decided 2026-07).
- phase-272 (per-node sched table), phase-273 (callback-group sched binding, RFC-0047),
  phase-274 (C/C++ → Model 1 convergence: native proven, FreeRTOS embedded proven).
- Convergence artifact: the unified-execution-model page (2026-07-07).
