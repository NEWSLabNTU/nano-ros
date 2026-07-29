# Phase 318 — Fixture freshness by toolchain output, and the tier ladder

**Status (2026-07-29): W1–W5 landed except W4.d (needs a cell→test-binary mapping).** Implements [RFC-0061](../design/0061-fixture-freshness-and-test-tiers.md).

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

- [x] **W1.a** `scripts/build/codegen-fingerprint.sh` — run `nros` over a
      committed probe corpus (one msg per configurable shape, one srv, one
      action), hash the emitted bytes. Cache at
      `.nros-cache/codegen-fingerprint/<sha256-of-binary>`; one probe run per new
      binary, a file read thereafter.
- [x] **W1.b** `scripts/build/resolve-fingerprint.sh` — same for
      `nros-launch-resolve` over a probe launch tree. Emitted SystemModels ARE
      fixture inputs, so a signature blind to the resolver repeats #182 one layer
      down. Falls back to the resolver's binary hash when CPython is unavailable
      (degrade to today's over-approximation, never to "assume fresh").
- [x] **W1.c** `workspace-fixture-signature.sh`: `tool:nros <binary-hash>` →
      `toolchain:<codegen_fp>[:<resolve_fp>]`, the resolver half included ONLY for
      records that declare a bringup. Signature version `v2` → `v3` (invalidates
      once, deliberately, never again for this reason).
- [x] **W1.d** Acceptance test: capture signatures → rebuild the CLI with a
      comment-only change → re-capture → **diff must be empty**. Inverse: a
      template edit that changes emitted bytes must invalidate the affected
      fixtures.

## W2 — the probe corpus doubles as a codegen golden test

- [x] **W2.a** Commit the corpus + its expected output under
      `packages/cli/rosidl-codegen/tests/`.
- [x] **W2.b** `tests/codegen_golden.rs` regenerates and diffs, reading the SAME
      `emit_corpus()` map the fingerprint hashes — a golden test covering
      different bytes than the fingerprint could pass while the fingerprint moved
      (or the reverse), and neither signal would be trustworthy. 28 golden files;
      re-record with `NROS_UPDATE_GOLDEN=1`. Also fails on ORPHANED goldens, so a
      file nothing emits any more cannot sit there asserting stale coverage.
      Original wording: a test that regenerates and diffs. Seconds, no fixture, no
      toolchain. Used ad hoc during 0344–0346 this pattern caught a ser/deser
      macro swap and a trailing-newline change that would have rewritten every
      generated file in the tree.

## W3 — tier selection computed from the matrix

- [x] **W3.a** `ci_lane::cells(CiLane::Tier1)` — `Native` ∩
      [1-wise(workload, kind) + pairwise(lang × rmw)]. Measured: **16 cells** of
      77 native. (RFC-0061 says 18, from a Python prototype; the shipped greedy
      tie-break finds a smaller cover for the same requirements — RFC updated.)
      Named `ci_lane`, not `tier`, because `matrix::Tier` already means
      Runtime / BuildOnly / CarveOut.
- [x] **W3.b** `ci_lane::cells(CiLane::Tier2)` — pairwise(platform × lang × rmw ×
      kind) over all Runtime cells. Measured: **37 cells**, 20 % of 182 — matching
      the RFC exactly.
- [x] **W3.c** Greedy set cover with a lexicographic tie-break so the chosen set is
      deterministic for a fixed cell table.
- [x] **W3.d** A test asserting each cover touches every declared value of every
      axis it pairs or singles — the regression that catches "someone added a
      platform and tier 2 silently skipped it".

## W4 — scope the staleness gate to the lane

- [x] **W4.a** `scripts/check-fixtures-stale.sh` takes a platform/cell filter.
- [x] **W4.b** Recipes: `ci` = tier 1, `ci-matrix` = tier 2, `ci-full` = today's
      `ci`. Each gates only its own fixtures — a native-intent run must not be
      blocked by a stale ThreadX fixture (which is exactly what happened).
- [ ] **W4.d** Wire the computed tier-2 cell set to the nextest filter. Today
      `ci-matrix` runs the full lane and says so — the selection exists
      (`nros_tests::ci_lane`) but nothing maps a cell to its test binary, so the
      filter cannot yet be derived from it. That mapping is the real work.
- [x] **W4.c** `CLAUDE.md` practice updated: "always `just ci`" points at tier 1,
      with tier 2 named for core/codegen/cmake changes. An instruction nobody can
      afford to follow gets followed selectively.

## W5 — operational corollaries (independent of W1–W4)

- [x] **W5.a** `scripts/build/drop-family-artifacts.sh` + `just sweep-family
      <platform> drop=1` — test one family, then free its manifest-declared build
      dirs. Dry-run by default, and it refuses to drop after an unclean run
      (you want those artifacts to debug). Measured on this tree: freertos alone
      is 12 dirs / 12.6 GB. **Partial by design** — the recipe owns test+drop, not
      build, because the per-platform build verbs differ (`just freertos
      build-fixtures` vs `just native build-fixtures` vs none for threadx). Fully
      interleaving the sweep means restructuring `build-test-fixtures`, which is
      its own change.
- [x] **W5.b** `.config/nextest.toml`: `qemu-emulated` test-group, `max-threads
      = 4`, covering the freertos/nuttx/threadx/zephyr/esp32/fvp/stm32 binaries
      (`qemu-baremetal` already capped its own family at 3). Verified accepted by
      `cargo nextest show-config test-groups`. The filter is the manual twin of
      `scripts/test/lane-filter.sh`'s derived tokens and must move with it.
- [x] **W5.c** Done — but by a parallel session, not here. `e7e5b84a0` (#328's
      fix) added `just test-ignored`; this phase wires it into `ci-full` so the
      lane actually runs. A `test-ignored-codegen` recipe drafted here was DELETED
      before commit: a second spelling of an existing verb is the exact
      anti-pattern this repo keeps paying for (CLAUDE.md "Fix the CLASS… add ONE
      shared helper rather than a second spelling"). Verified green: 12/12 in
      rosidl-codegen.

---

## Acceptance

Per RFC-0061 §Acceptance. The load-bearing ones:

- [x] A `just setup-cli` that does not change emitted bytes invalidates **zero**
      workspace fixtures. *(W1.d acceptance, both arms: no-op rebuild → 0 of 81
      invalidated; one-line template edit → fingerprint moves and fixtures
      invalidate.)*
- [x] A resolver rebuild that does not change emitted models invalidates zero;
      one that does invalidates the bringup-bearing fixtures and no others.
      *(W1.b acceptance: a no-op resolver rebuild moved the binary hash
      `e042c59a…` → `dcfef93f…` while the fingerprint stayed `26ac1134…`; a
      changed probe model moved it to `29cd5817…`. Scoping note: every workspace
      record declares a bringup today, so the scope is a no-op in practice —
      it is the right contract, not a current reduction.)*
- [ ] `just ci` (tier 1) runs to completion with a stale ThreadX fixture on disk.
      *(Scope reduction demonstrated — the gate drops from 81 to 65 workspace
      records, excluding exactly the 16 freertos/nuttx/threadx ones that blocked
      the 2026-07-28 run — but a full tier-1 run has not been timed yet.)*
- [x] Tier 1/2 covers are computed, not listed; adding a platform to
      `matrix::CELLS` extends tier 2 with no second edit. *(Gated by
      `ci_lane::tests::lanes_touch_every_declared_value_of_every_axis_they_cover`
      and `lane_filter_tokens_cover_every_non_native_platform`.)*

## Non-goals

- Reproducible Rust binaries (the fingerprint sidesteps the question).
- Replacing cargo/ninja fingerprinting for rust/cmake fixtures — those are exact
  and self-healing already.
- Removing `NROS_SKIP_FIXTURE_CHECK=1`. It stays; the aim is that it stops being
  routine.
