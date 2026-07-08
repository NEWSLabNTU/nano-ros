# Phase 284 — Example fixture coverage completion (resolve #102)

Status: **In progress — 2026-07-09** · Resolves issue #102 · Follows
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
- [ ] W2.b Add a runtime e2e (or a justified compile-only de-scope) for each
  remaining native variant W1 confirms uncovered. Prefer asserting the variant's
  DISTINGUISHING behavior (async completion, component wiring, custom-msg
  round-trip), not a bare severity dump.

### W3 — H1 residual: embedded-capability spot-checks
Phase-276 proved lifecycle/params/safety/QoS/multihost on Zephyr native_sim.
- [ ] W3.a Check whether any of the five capabilities remains native-only on the
  OTHER embedded platforms (freertos/nuttx/threadx) after 276. If the RFC-0026
  matrix intends one embedded proof per capability (already met by Zephyr),
  de-scope the rest with that reason; else file the specific missing cells.

### W4 — H5: threadx-riscv64 cyclonedds svc/action
- [ ] W4.a The cyclone RMW variant is talker+listener only (svc/action exist
  under zenoh). Add the cyclone svc/action fixture rows, or de-scope as an
  RMW-coverage (not example) gap with the reason.

### W5 — H6: stale examples, fix-or-delete
- [ ] W5.a `examples/px4/rust/uorb` — README placeholder (Rust crate deleted
  Phase 115.K.4; C++ canonical): delete the dir or convert to a real pointer.
- [ ] W5.b `examples/zephyr/rust/service-client-async` — dropped from the matrix,
  dir never removed (shape-tested only): delete or re-add to the matrix.
- [ ] W5.c `examples/stm32f4/rust/{talker,listener}-embassy` — `talker-embassy`
  compile-checked but `skip_build=true`; `listener-embassy` uncovered. Decide:
  cover, un-skip, or de-scope with reason.

### W6 — Close #102
- [ ] W6.a Every hole is covered or de-scoped-with-reason; the per-wave check
  jobs return empty (or list only the recorded de-scopes).
- [ ] W6.b `status: resolved`, `resolved_in:` this phase (+ 275/276), move #102 to
  `docs/issues/archived/`; refresh the issues README index.

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
