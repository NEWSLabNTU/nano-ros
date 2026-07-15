# Phase 284 — Example fixture coverage completion (resolve #102)

Status: **Complete — 2026-07-09** · Resolves issue #102 · Follows
[phase-275](archived/phase-275-example-fixture-gap-fill.md) (H2–H6 mechanical
gap-fill, Complete) + [phase-276](archived/phase-276-capability-coverage-on-embedded.md)
(H1 embedded capabilities, Complete) · Informs RFC-0026 (examples).

> **Goal.** Drive #102 (example fixture coverage holes) to a clean `resolved`:
> every example either has a fixture/e2e that exercises it, OR is explicitly
> de-scoped with a one-line reason ("no silent caps"). Phases 275/276 landed the
> bulk (H1 on embedded; H2–H6 mechanical); this phase reconciles the STALE
> 2026-07-01 hole inventory against the current tree, closes the residual, and
> updates + archives the issue.

## Why

#102's remaining-holes list is dated **2026-07-01** and is stale: phase-276
(2026-07-04) closed H1 (lifecycle/params/safety/QoS/multihost on Zephyr
native_sim), phase-275 (2026-07-08) did the H2–H6 mechanical sweep, and phase-284
already added the first native-variant runtime e2e (`native/rust/logging`,
`a43af7bdd`). But the issue text still lists all of H1–H6 as open, so #102 reads
as a large open tracker when most of it is done. The issue can't be closed on an
inaccurate inventory — this phase re-audits, closes the true residual, and
records which phase owns each hole.

## Method — CHECK (re-derive) then FIX (cover or de-scope)

The tree moved under phases 275/276/277/281/283, so never trust the 2026-07-01
inventory: every wave first re-derives the CURRENT coverage for its slice
(grep the example dir vs `examples/fixtures.toml` + the driver scripts + the
`tests/` consumers), then either adds a fixture row + a runtime e2e that asserts
BEHAVIOR (not just "it built"), or DE-SCOPES the cell with a one-line reason in
the issue/README. A hole is "closed" only when the check is empty AND (covered:
the e2e passes) or (de-scoped: the reason is recorded — never a silent cap).

## Waves

### W1 — Reconcile the inventory (audit; do first) — DONE 2026-07-09
- [x] W1.a Re-derived vs the current tree. H1→phase-276 (closed). H2→phase-275
  (`_entry`→`-entry` rename) + build-asserts + nuttx/freertos runtime; the 18
  `_entry` dirs are untracked build-junk leftovers. H3 mostly covered (custom-msg,
  c/cpp/rust logging); H4 `talker-typed` gone; H6 `px4/uorb` gone.
- [x] W1.b Rewrote #102's "Remaining holes" as a reconciled 2026-07-09 section
  (old 07-01 list kept as "Original … superseded").
- [x] W1.c Residual work-list for W2–W5:
  - **W2 (H3):** runtime e2e (or de-scope) for `native/cpp/{component-poc,
    component-node-poc, transform-poc}`, `native/rust/{action-client-async,
    service-client-async}`.
  - **W3 (H1):** de-scope-with-reason the non-Zephyr embedded capability cells
    (RFC-0026 = one embedded proof; Zephyr met it) OR file specific gaps.
  - **W4 (H4/H5):** `zephyr/{cpp,rust}/cyclonedds` leaves + threadx-riscv64
    cyclone svc/action (RMW-scoped).
  - **W5 (H6 + H2 junk):** delete the 18 untracked `_entry` leftover dirs;
    fix-or-delete `zephyr/rust/service-client-async` + stm32f4 embassy pair.

### W2 — H3 residual: native-variant runtime e2e
Native example variants with a fixture row but no runtime e2e (they only
compile-check). `native/rust/logging` is DONE (`a43af7bdd`). Remaining candidates
(confirm in W1): `native/c/logging`, `native/cpp/logging` (cmake + a zenoh
session), `native/rust/{action-client-async,service-client-async}`,
`native/{c,cpp}/{component-poc,component-node-poc,transform-poc,custom-msg}`.
- [x] W2.a `native/rust/logging` — runtime e2e (`native_rust_logging_example_
  threshold_raise_filters_round_two`) proves the runtime `set_level` filter the
  logging-smoke bins don't cover. Landed `a43af7bdd`.
- [x] W2.b COVERED the rust async clients: `native_async_roundtrip_e2e`
  (`b3f645f7a`) pairs `native/rust/{service,action}-client-async` with the sync
  native servers over a private zenohd and asserts the awaited Promise resolves
  (tokio `spin_async` + `.await` — the distinguishing behavior). 2/2 green.
  DE-SCOPED (compile-check row is sufficient; no silent cap — reason recorded):
  - `native/cpp/{component-poc, component-node-poc}` — the C++ component
    registration + spin model is runtime-proven by the cpp workspace-entry e2e
    (`cpp_multi_node_entry` + the component roundtrips); these standalone POCs
    duplicate that path.
  - `native/cpp/transform-poc` — a tf-style POC with no runtime transform-
    assertion harness; compile-check is the proportionate tier.

### W3 — H1 residual: embedded-capability spot-checks — DE-SCOPED 2026-07-09
Phase-276 proved lifecycle/params/safety/QoS/multihost on Zephyr native_sim.
- [x] W3.a De-scoped with reason (no silent cap). RFC-0026's coverage matrix
  intends ONE embedded proof per capability — to show the capability works off
  native, not to re-run all five on every RTOS. Zephyr native_sim (phase-276)
  satisfies that for all five; RT-tiers additionally reaches FreeRTOS/NuttX
  (`orchestration_tiers_freertos`, `realtime_tiers_{c,cpp,rust}_nuttx_e2e`). A
  freertos/nuttx/threadx re-proof of lifecycle/params/safety/QoS/multihost is
  redundant matrix fill, not a coverage gap; deferred unless a specific
  platform-specific capability defect motivates it.

### W4 — H4/H5: cyclone-RMW svc/action + zephyr cyclone leaves — DE-SCOPED 2026-07-09
- [x] W4.a De-scoped with reason (no silent cap). These are RMW-transport-matrix
  cells, not example gaps: the service + action EXAMPLES themselves are runtime-
  proven under the PRIMARY zenoh RMW (`*_service_roundtrip_*`,
  `*_action_roundtrip_*`, threadx-riscv64 zenoh svc/action), and cyclone
  talker+listener proves the cyclone transport delivers. Adding cyclone svc/action
  + `zephyr/{cpp,rust}/cyclonedds` leaf fixtures is second-RMW coverage that needs
  the heavy embedded cyclone build lanes; deferred as lower-priority transport-
  matrix fill (tracked here), not an example-coverage gap that blocks #102.

### W5 — H6 + H2 junk: stale examples — RESOLVED 2026-07-09
- [x] W5.a `examples/px4/rust/uorb` — already GONE (deleted before this phase).
- [x] W5.b `examples/zephyr/rust/service-client-async` — NOT tracked by git
  (0 tracked files); it is an untracked build-artifact leftover, not a tracked
  example. Same class as the 18 `examples/*/rust/*_entry/` dirs the phase-275
  `_entry`→`-entry` rename left behind (`generated/`+`target/` only). These are
  `.gitignore`d and removed by `just clean` — a local-workspace concern, not a
  repo change or a coverage gap. No commit needed.
- [x] W5.c stm32f4 embassy — DE-SCOPED with reason (no silent cap):
  `talker-embassy` is compile-checked (the `embassy_main!` macro path via
  `stm32f4_embassy_main_macro.rs`; `skip_build=true` because the embassy build
  needs a pinned toolchain the fixture matrix doesn't carry). `listener-embassy`
  is a redundant SECOND demo of the same compile-proven macro path — no distinct
  runtime/compile surface, so compile-check parity (not a runtime e2e) is the
  proportionate tier; left as-is rather than adding a duplicate.

### W6 — Close #102 — DONE 2026-07-09
- [x] W6.a Every EXAMPLE is covered or de-scoped-with-reason (no silent caps):
  COVERED — H1 (phase-276), H2 build-asserts + nuttx/freertos runtime (phase-275/
  281/130), H3 custom-msg + c/cpp/rust logging + rust async (W2). DE-SCOPED with
  recorded reasons — H3 cpp POCs (proven by cpp workspace entry e2e), H1 non-Zephyr
  matrix fill (RFC-0026 = one embedded proof), H4/H5 cyclone svc/action + zephyr
  cyclone leaves (secondary-RMW transport-matrix, examples proven under zenoh),
  H6 embassy listener (redundant compile demo). The untracked `_entry` / zephyr
  async leftover dirs are `.gitignore`d build junk (`just clean`), not gaps.
- [x] W6.b #102 `status: resolved`, `resolved_in: phase-284` (+ 275/276), moved to
  `docs/issues/archived/`; issues README index refreshed.

## Status: Complete — 2026-07-09.

## Non-goals

- New example CONTENT or platforms (that is RFC-0026 + the per-platform phases) —
  this phase only closes the coverage/exercise gap on examples that already exist.
- Re-doing 275/276's landed work; W1 credits them and moves on.
- The `_entry` → `-entry` directory rename (phase-275 territory / #136 item 4).

## Acceptance

- #102's hole inventory reflects the current tree (W1).
- Every residual example is covered by a passing e2e OR de-scoped with a recorded
  one-line reason ("no silent caps").
- `just format` + the touched-test lanes green.
- #102 `resolved` + archived.

## Sequencing

W1 first (the audit gates everything — the 2026-07-01 list is stale). W2–W5 are
independent slices runnable in any order against W1's residual list. W6 closes
the issue once the check jobs are empty.
