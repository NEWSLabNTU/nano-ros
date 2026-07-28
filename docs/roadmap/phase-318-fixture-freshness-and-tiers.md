# Phase 318 — Fixture freshness by toolchain output, and the tier ladder

**Status (2026-07-28): W1 in progress.** Implements [RFC-0061](../design/0061-fixture-freshness-and-test-tiers.md).

**Why now.** A `just ci` run on 2026-07-28 passed every code stage and was then
blocked by 40 "stale" workspace fixtures, none of which were semantically stale —
a multi-hour, ~100 GB rebuild whose correct answer was zero. The trigger was a
codegen change that only ADDED a rejection path no in-tree fixture uses. Same run
hit 11 MB free twice.

---

## W1 — toolchain fingerprint replaces the tool binary hash

The workspace-fixture signature hashes `sha256(packages/cli/target/release/nros)`
(issue #182, for a real reason: a museum emitter once verified as fresh). Rust
binaries are not reproducible across rebuilds, so the hash moves whenever the CLI
is rebuilt, whether or not emitted bytes changed.

- [ ] **W1.a** `scripts/build/codegen-fingerprint.sh` — run `nros` over a
      committed probe corpus (one msg per configurable shape, one srv, one
      action), hash the emitted bytes. Cache at
      `.nros-cache/codegen-fingerprint/<sha256-of-binary>`; one probe run per new
      binary, a file read thereafter.
- [ ] **W1.b** `scripts/build/resolve-fingerprint.sh` — same for
      `nros-launch-resolve` over a probe launch tree. Emitted SystemModels ARE
      fixture inputs, so a signature blind to the resolver repeats #182 one layer
      down. Falls back to the resolver's binary hash when CPython is unavailable
      (degrade to today's over-approximation, never to "assume fresh").
- [ ] **W1.c** `workspace-fixture-signature.sh`: `tool:nros <binary-hash>` →
      `toolchain:<codegen_fp>[:<resolve_fp>]`, the resolver half included ONLY for
      records that declare a bringup. Signature version `v2` → `v3` (invalidates
      once, deliberately, never again for this reason).
- [ ] **W1.d** Acceptance test: capture signatures → rebuild the CLI with a
      comment-only change → re-capture → **diff must be empty**. Inverse: a
      template edit that changes emitted bytes must invalidate the affected
      fixtures.

## W2 — the probe corpus doubles as a codegen golden test

- [ ] **W2.a** Commit the corpus + its expected output under
      `packages/cli/rosidl-codegen/tests/`.
- [ ] **W2.b** A test that regenerates and diffs. Seconds, no fixture, no
      toolchain. Used ad hoc during 0344–0346 this pattern caught a ser/deser
      macro swap and a trailing-newline change that would have rewritten every
      generated file in the tree.

## W3 — tier selection computed from the matrix

- [ ] **W3.a** `matrix::tier1_cells()` — `Native` ∩
      [1-wise(workload, kind) + pairwise(lang × rmw)]. Measured: **18 cells** of 77 native.
- [ ] **W3.b** `matrix::tier2_cells()` — pairwise(platform × lang × rmw × kind)
      over all Runtime cells. Measured: **37 cells**, 20 % of 182.
- [ ] **W3.c** Greedy set cover with a lexicographic tie-break so the chosen set is
      deterministic for a fixed cell table.
- [ ] **W3.d** A test asserting each cover touches every declared value of every
      axis it pairs or singles — the regression that catches "someone added a
      platform and tier 2 silently skipped it".

## W4 — scope the staleness gate to the lane

- [ ] **W4.a** `scripts/check-fixtures-stale.sh` takes a platform/cell filter.
- [ ] **W4.b** Recipes: `ci` = tier 1, `ci-matrix` = tier 2, `ci-full` = today's
      `ci`. Each gates only its own fixtures — a native-intent run must not be
      blocked by a stale ThreadX fixture (which is exactly what happened).
- [ ] **W4.c** `CLAUDE.md` practice updated: "always `just ci`" points at tier 1,
      with tier 2 named for core/codegen/cmake changes. An instruction nobody can
      afford to follow gets followed selectively.

## W5 — operational corollaries (independent of W1–W4)

- [ ] **W5.a** Tier 3 drops each family's artifacts after that family passes. A
      full sweep needs ~800 GB and hit 11 MB free twice on 2026-07-28; the
      artifacts are reproducible, the result is what needs keeping.
- [ ] **W5.b** QEMU concurrency cap for tier 3 (287-W7: six NuttX lanes failed
      in-sweep, passed solo). A tier whose reds are routinely noise trains people
      to ignore reds.
- [ ] **W5.c** Issue 0328's 24 `#[ignore]` tests get a tier or get deleted. Note
      0345 repaired the rotted stubs, so a lane added now would be green —
      before that it would have gone red on arrival, which is why nobody added one.

---

## Acceptance

Per RFC-0061 §Acceptance. The load-bearing ones:

- [ ] A `just setup-cli` that does not change emitted bytes invalidates **zero**
      workspace fixtures.
- [ ] A resolver rebuild that does not change emitted models invalidates zero;
      one that does invalidates the bringup-bearing fixtures and no others.
- [ ] `just ci` (tier 1) runs to completion with a stale ThreadX fixture on disk.
- [ ] Tier 1/2 covers are computed, not listed; adding a platform to
      `matrix::CELLS` extends tier 2 with no second edit.

## Non-goals

- Reproducible Rust binaries (the fingerprint sidesteps the question).
- Replacing cargo/ninja fingerprinting for rust/cmake fixtures — those are exact
  and self-healing already.
- Removing `NROS_SKIP_FIXTURE_CHECK=1`. It stays; the aim is that it stops being
  routine.
