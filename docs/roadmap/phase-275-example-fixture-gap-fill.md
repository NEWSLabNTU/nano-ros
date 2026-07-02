# Phase 275 — Example fixture gap-fill (mechanical coverage holes)

Status: **In progress (2026-07-02)** · Implements issue #102 (H2–H6) · Informs RFC-0026 (examples).

**Progress (2026-07-02, verified on a known-good machine):** W2 (native all-lang variants) ✓ ·
W3 (zephyr non-role leaves — no real gap, FVP-covered) ✓ · W4 (threadx-riscv64 cyclone svc/action —
de-scoped) ✓ · W5 (stm32 listener-embassy) ✓ · W6 (silent-gap gate `examples_fixture_coverage.rs`) ✓.
W1 partial: freertos entries already run-plan-covered, threadx-linux entries **landed** (fixture rows +
`threadx_linux_entry_build.rs`); **nuttx entries blocked** on a standalone-`[[bin]]` NuttX-libc link
gap (issue #127). Working detail → `phase-275-276-branch-notes.md`.

> **Goal.** Close the *mechanical* example-coverage holes from the 2026-07-01 re-audit of
> issue #102: examples that exist in-tree (and are claimed in the RFC-0026 matrix) but are
> never built or tested by any fixture mechanism. Each hole is resolved by **either** adding a
> fixture row (+ a behavior/build assertion) **or** honestly de-scoping the matrix cell — never
> leaving a matrix-listed example that CI never builds ("no silent caps"). This phase is the
> small, mostly-mechanical set; the large capability-on-embedded work is **Phase 276**.

## Why (2026-07-01 re-audit of #102)

The original #102 snapshot (2026-06-26) claimed ~60 untested examples. Re-audit against the
current tree found the **P0 gaps already closed** (Zephyr single-node examples are built by
`scripts/build/zephyr-fixture-leaves.sh`; FreeRTOS/NuttX C/C++ have cmake rows in
`examples/fixtures.toml`). What remains splits into five holes; H2–H6 are addressed here, H1
(capability-on-embedded) in Phase 276.

Fixture mechanisms in play (a fix lands in whichever one fits the platform):
`examples/fixtures.toml` `[[fixture]]` rows · the Zephyr `zephyr-fixture-leaves.sh` +
`fixture-matrix.sh` role matrix · `scripts/build/compile-check-fixtures.sh` · test-driven
builders (`freertos_run_plan_runtime.rs`, `phase_118_collapse.rs`).

## Work items

### W1 — `*_entry` demo coverage (#102 H2)
18 per-example entry demos exist —
`examples/{qemu-arm-freertos,qemu-arm-nuttx,threadx-linux}/rust/{talker,listener,service-server,
service-client,action-server,action-client}_entry` — but only freertos `talker_entry` is built/run
(by `freertos_run_plan_runtime.rs`). The other 17 have no fixture or test.
- **Do:** add fixture rows (or extend the run-plan test harness) so every `*_entry` is at least
  build-asserted; where a platform already has a runtime harness (freertos run-plan), extend it to
  the sibling roles.
- **Acceptance:** each of the 18 `*_entry` dirs is built by a fixture; freertos entry demos run
  (not just build) via the run-plan harness; nuttx/threadx entry demos build-assert at minimum.
  Any that can't be supported yet are `skip_build`/`skip_probe` with a tracked reason, not silent.

### W2 — native variant examples (#102 H3)
0-fixture native examples: `native/c/{custom-msg,custom-platform,custom-transport-loopback,logging}`,
`native/cpp/{component-poc,component-node-poc,transform-poc,logging}`,
`native/rust/{action-client-async,service-client-async,logging}`.
- **Do:** add `examples/fixtures.toml` rows; for the ones with observable behavior (async
  service/action clients, custom-msg round-trip) add a runtime assertion under `nros-tests/tests/`;
  build-assert the POCs/logging.
- **Acceptance:** every listed native example has a fixture; async + custom-msg have a runtime test.

### W3 — Zephyr non-role leaves (#102 H4)
`zephyr/cpp/{cyclonedds,talker-typed}`, `zephyr/rust/{cyclonedds,service-client-async}` are outside
the 6-role driver matrix and unbuilt.
- **Do:** extend the `fixture-matrix.sh` role/variant set to include them, **or** de-scope from the
  matrix if redundant with a role example.
- **Acceptance:** each leaf is built by the zephyr driver or explicitly de-scoped in
  `examples/README.md` with a note.

### W4 — threadx-riscv64 cyclonedds svc/action (#102 H5)
threadx-riscv64 svc/action are built under **zenoh** (`fixtures.toml` ~2408–2478) but the
**cyclonedds** RMW variant is talker+listener only (~2560–2593).
- **Do:** mirror the zenoh svc/action rows for cyclonedds.
- **Acceptance:** threadx-riscv64 cyclonedds has service + action fixtures matching the zenoh set.

### W5 — stale examples: fix-or-delete (#102 H6)
- `examples/px4/rust/uorb` — README placeholder (Rust crate deleted Phase 115.K.4). **Delete** the
  dir + drop the matrix cell; C++ uORB is canonical.
- `examples/zephyr/rust/service-client-async` — dropped from the matrix but dir never removed;
  delete or re-scope (overlaps W3).
- `examples/stm32f4/rust/listener-embassy` — fully uncovered; fix (add compile-check like
  `talker-embassy`) or delete. Re-check whether `talker-embassy`'s `skip_build=true`
  (`fixtures.toml` ~1600) can now be flipped.
- **Acceptance:** no in-tree example is matrix-listed but unbuildable-and-untracked; each stale dir
  is deleted or has a fixture.

### W6 — close the silent-gap loophole
Extend `examples_canonical_shape.rs` (and/or `example_shape.rs`) so a matrix cell **without** a
fixture is a **tracked exception** (explicit allowlist with reason), failing the test if a new
example appears with neither a fixture nor an exception entry.
- **Acceptance:** adding a matrix example with no fixture and no exception fails a shape test.

## Sequencing

W5 (delete stale — cheapest, shrinks the surface) → W4 (mirror rows) → W3 (matrix extend) →
W1 (`_entry` rows) → W2 (native variants) → W6 (gating, last so it ratchets the cleaned state).

## Constraints

Adding fixtures requires build-verification on a **known-good machine** — the current dev host has
failing RAM (issue #115), so builds there are untrustworthy. Verify all fixture additions elsewhere.

## Cross-links

Issue #102 (H2–H6) · Phase 276 (capability-on-embedded, #102 H1) · RFC-0026 (examples matrix) ·
Phase 263 (workspace examples — the workspace axis, distinct from these single-node/entry holes).
