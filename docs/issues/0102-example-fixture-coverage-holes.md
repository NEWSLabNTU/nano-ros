---
id: 102
title: "Example fixture coverage holes — capability-on-embedded, native variants, and per-example `_entry` demos still untested"
status: open
type: tech-debt
area: testing
related: [rfc-0026, phase-263, phase-267, phase-275, phase-276]
---

## Status — re-audited 2026-07-01 (P0 largely resolved)

The original 2026-06-26 snapshot ("~60 examples untested; Zephyr 22 / FreeRTOS 12 / NuttX 12 with
zero single-node fixtures") is now **substantially stale**. A full re-audit against the current
tree — cross-checking `examples/fixtures.toml`, the Zephyr fixture driver, per-platform drivers,
`compile-check-fixtures.sh`, and `packages/testing/nros-tests/tests/` — found the big P0 gaps
**closed**. What remains is a smaller, sharper set of holes (below). The original snapshot is kept
at the end for history.

**Key correction:** single-node fixtures come from *several* mechanisms, not just
`examples/fixtures.toml`:
- `examples/fixtures.toml` `[[fixture]]` rows — native/baremetal/esp32/stm32/**freertos & nuttx
  C/C++**/threadx single-node cells.
- **Zephyr driver** `scripts/build/zephyr-fixture-leaves.sh` + `scripts/build/fixture-matrix.sh` —
  builds `examples/zephyr/{c,cpp,rust}/{talker,listener,service-{server,client},action-{server,client}}`
  × {zenoh,xrce,+cyclonedds}, consumed by `zephyr.rs` / `phase_118_collapse.rs`. (This is why the
  "Zephyr 0 fixtures" claim was wrong — the coverage just isn't in `fixtures.toml`.)
- `scripts/build/compile-check-fixtures.sh` — `orch_tiers_freertos`, `stm32f4/rust/talker-embassy`.
- Test-driven builders — `freertos_run_plan_runtime.rs` (freertos `talker_entry`),
  `phase_118_collapse.rs`, the phase-263 `*_entry_e2e.rs` / workspace `*_e2e.rs`.

### P0 — DONE
- **Zephyr** c/cpp/rust × 6 roles × zenoh/xrce(/cyclone) built by the leaves driver. (Was "22
  examples, 0 fixtures".) Only 4 non-role leaves remain — see below.
- **FreeRTOS C/C++ (12)** — all present, `examples/fixtures.toml` ~2163–2233.
- **NuttX C/C++ (12)** — all present, `examples/fixtures.toml` ~2240–2310.

## Phases

The remaining holes are planned across two roadmap phases:
- **[Phase 275](../roadmap/phase-275-example-fixture-gap-fill.md)** — mechanical gap-fill: **H2–H6**
  (`_entry` demos, native variants, zephyr leaves, threadx cyclone, stale cleanup, shape-gating).
- **[Phase 276](../roadmap/archived/phase-276-capability-coverage-on-embedded.md)** — **H1**:
  lifecycle / parameters / safety / QoS / multihost on embedded targets. **DONE 2026-07-04** —
  all six waves (incl. RT-tiers) proven on Zephyr native_sim e2e; H1 is closed.

## Remaining holes (2026-07-01, priority order)

**H1 — capabilities exercised on `native` only** (the truest remaining core). No embedded fixture
exercises **lifecycle, parameters, safety/CRC, QoS-overrides, or multihost**. Each has exactly one
native fixture. *Progress:* RT-tiers now reaches FreeRTOS (`orch_tiers_freertos` +
`orchestration_tiers_freertos.rs`); basic pub/sub **workspace-entry** e2e reaches zephyr/freertos/
nuttx/threadx (phase-263 C2x). But the five capabilities above remain native-only.

**H2 — per-example `*_entry` demos unexercised (new; not in the original snapshot).** 18 dirs —
`examples/{qemu-arm-freertos,qemu-arm-nuttx,threadx-linux}/rust/{talker,listener,service-server,
service-client,action-server,action-client}_entry` — exist but only freertos `talker_entry` is
built/run (by `freertos_run_plan_runtime.rs`). The other 17 have no dedicated fixture or test.

**H3 — native variant examples, 0 fixtures:**
- native/c: `custom-msg`, `custom-platform`, `custom-transport-loopback`, `logging`
- native/cpp: `component-poc`, `component-node-poc`, `transform-poc`, `logging`
- native/rust: `action-client-async`, `service-client-async` (`logging` now has a runtime
  e2e — `native_rust_logging_example_threshold_raise_filters_round_two`, proves the runtime
  `set_level` filter the smoke bins don't cover)

**H4 — Zephyr non-role leaves, 0 fixtures:** `zephyr/cpp/{cyclonedds,talker-typed}`,
`zephyr/rust/{cyclonedds,service-client-async}`.

**H5 — threadx-riscv64 cyclonedds is talker+listener only** (svc/action now built under **zenoh**,
`fixtures.toml` ~2408–2478; the cyclone RMW variant, ~2560–2593, still lacks svc/action). RMW-scoped,
not example-scoped.

**H6 — stale examples to fix-or-delete:**
- `examples/zephyr/rust/service-client-async` — dropped from the matrix but dir never removed;
  shape-tested only.
- `examples/px4/rust/uorb` — README placeholder (Rust crate deleted Phase 115.K.4; C++ canonical).
- `examples/stm32f4/rust/{talker,listener}-embassy` — `talker-embassy` now compile-checked
  (`embassy_main_macro` + `stm32f4_embassy_main_macro.rs`), but `skip_build=true` in `fixtures.toml`
  and `listener-embassy` is fully uncovered.

## Fix direction (unchanged principle)

Per hole: either (a) add a fixture row (`examples/fixtures.toml` or the relevant driver matrix) +
a behavior test under `nros-tests/tests/`, or (b) honestly de-scope the cell from the RFC-0026
matrix. Do NOT leave a matrix-listed example that CI never builds — that reads as "covered" when it
isn't ("no silent caps"). Extend `examples_canonical_shape.rs` gating so a matrix cell without a
fixture is a *tracked exception*, not a silent gap.

Sequence: **H2** (mechanical — the `_entry` demos already exist, just need fixture rows) → **H6**
(stale cleanup, cheap) → **H3** (native variants) → **H5** (threadx cyclone RMW) → **H4** (zephyr
leaves) → **H1** (capability-on-embedded — the largest, needs new per-capability embedded fixtures).

> **Note:** adding fixtures requires build-verification on a **known-good machine**. The current dev
> host has failing RAM (see issue #115) — builds there are untrustworthy, so fixture work must be
> validated elsewhere. This re-audit is read-only and unaffected.

## Original snapshot (2026-06-26, superseded — kept for history)

A 2026-06-26 audit cross-checking `examples/README.md` against the tree + `examples/fixtures.toml`
+ `nros-tests/tests/` reported ~60 example projects claimed in the RFC-0026 matrix but never
built+tested: Zephyr 22 / FreeRTOS C/C++ 12 / NuttX C/C++ 12 single-node examples with zero
fixtures, plus native variants, native Rust async, and capabilities exercised on native only. The
Zephyr/FreeRTOS/NuttX single-node claims are now resolved (see above); the audit undercounted by
missing the Zephyr driver and the `*_entry` surface.
