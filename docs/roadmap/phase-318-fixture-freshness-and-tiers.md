# Phase 318 — Fixture freshness by toolchain output, and the tier ladder

**Status (2026-07-30): COMPLETE — W1–W5 landed. W4.d's measurement split tier 2 into a 1-wise `ci-matrix` and a pairwise `ci-matrix-nightly` (RFC-0061 decision 3).** Implements [RFC-0061](../design/0061-fixture-freshness-and-test-tiers.md).

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

### W1.b addendum (2026-07-30, phase-315 session)

`resolve-fingerprint.sh` caches on `sha256(nros-launch-resolve)`, which makes
binary freshness a PRECONDITION rather than something it can verify: a museum
binary has a stable hash, so it emits a stable fingerprint and every fixture is
reported fresh indefinitely. W1.b's own reasoning — "a signature blind to the
resolver repeats #182 one layer down" — extends one layer further, to a probe
blind to the resolver's SOURCES.

That was live, briefly. `just setup-launch-resolve`'s staleness probe had been
converted from `find` to `git ls-files`, and `git -C <outer> ls-files` lists
only the inner gitlink for a NESTED submodule — ros-launch-manifest sits inside
ros-launch-resolve. An edit there left a binary a full day older than its
source with the probe reporting fresh; the symptom was a resolver fix that
appeared not to work. Fixed with `--recurse-submodules`; cross-referenced in
both files so the halves are read together.

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
- [x] **W4.b** Recipes: `ci` = tier 1, `ci-matrix` = tier 2, `ci-matrix-nightly`
      = the pairwise cover (added in W4.d), `ci-full` = today's `ci`. Each gates only its own fixtures — a native-intent run must not be
      blocked by a stale ThreadX fixture (which is exactly what happened).
- [x] **W4.d** Wire the computed lane selection to the lane. **Done — and the
      measurement that came out of it split tier 2 in two.**

      | lane | selection | cells | coords | cost |
      | --- | --- | --- | --- | --- |
      | tier 1 | native, 1-wise(w,k) + pairwise(l × r) | 16 | 10 | 21 % |
      | tier 2 `ci-matrix` | 1-wise(p, l, r, k) | 11 | 12 | 26 % |
      | tier 2n `ci-matrix-nightly` | pairwise(p × l × r × k) | 37 | 33 | 70 % |
      | tier 3 `ci-full` | everything | 182 | 47 | 100 % |

      The work item assumed the saving was in test SELECTION. It is not: every
      cover touches all ten platforms by construction (W3.d's anti-rot gate
      requires every declared value of every covered axis), so a per-platform
      nextest filter excludes nothing. The saving is entirely in which FIXTURES
      get built — hence coordinates, not cells, as the unit.

      And in that unit RFC-0061's "tier 2 ≈ 20 % of a sweep" was 70 %: cells
      share fixtures (the four threadx-linux C cyclonedds cells are one build), so
      an 80 % cell reduction is a 30 % build reduction. The floor is structural —
      pairwise(platform × lang) is 29 fixtures because there are 29 declared pairs
      — so the only lever was whether tier 2 pairs platform × lang at all, which
      is the class (0268, 0245, 0332) the tier exists to catch. Split rather than
      chosen between: `ci-matrix` is the affordable 1-wise gate, `ci-matrix-nightly`
      keeps the pairwise coverage a day later. RFC-0061 decision 3.

      Shipped:
      - `lane-coords <tier1|tier2|tier2-nightly>` → the lane's `platform,lang,rmw`
        coordinates; `--cells` for reading.
      - `fixtures-manifest.py --coords-from FILE`; an empty/absent set is a hard
        error, never a silent select-nothing (which would make a broken lane look
        instant).
      - `NROS_FIXTURE_SCOPE=coords` + `NROS_FIXTURE_COORDS` in
        `check-fixtures-stale.sh`, and `just _lane-gate <lane>` feeding gate and
        build from ONE `lane-coords` invocation so they cannot disagree.
      - `PlatformId::fixture_tokens` / `from_fixture_token` in `matrix.rs` as the
        SSoT for the fixtures.toml platform vocabulary, with a round-trip gate.
        This mapping had existed only inside `tests/matrix_fixture_coverage.rs`;
        writing `coords` produced a second, DISAGREEING copy of the forward
        direction (`qemu-esp32-baremetal` attributed to `QemuBaremetal` instead of
        `Esp32Qemu`) — caught by a new test on its first run, and consolidated
        rather than corrected in place.

      Verified: `just _lane-gate tier2` gates 12 coordinates and reports 5 stale
      workspace fixtures, all inside the selection; the unscoped gate covers 81
      records.

- [x] **W4.e** `ci-matrix-nightly` wired into `.github/workflows/nightly.yml`
      (07:00 cron). It cannot run as one CI job — the fixture build scripts need
      per-platform toolchain env that only the `just <module>` recipes export, and
      the cover spans eight modules whose SDKs do not coexist on one runner — so
      it runs distributed:
      - `lane` computes the cover and publishes `lane-coords.txt` /
        `lane-modules.txt` as an artifact plus a job summary;
      - `changes` DERIVES the platform matrix from it. That list was hand-written
        (`all="qemu freertos …"`), so adding a platform to `matrix::CELLS` used to
        extend the lane on paper and nothing on the runners — the exact rot
        `ci_lane` exists to prevent, reintroduced one layer out in a workflow yml;
      - `lane-coverage` asserts every lane module has a job SOMEWHERE in nightly
        CI (`native` → host-tests.yml 03:00, `zephyr` → the 05:00 cron, the rest →
        the platform matrix), and fails if `lane` itself failed — an empty
        selection skips the sweep silently, and a nightly that ran nothing looks
        exactly like one that passed.

      Blast radius was the design constraint: `lane` is its own job rather than a
      step in `changes`, because `changes` also gates the Zephyr cron and a cargo
      failure must not take down jobs that never needed the lane. `lane-coverage`
      is excluded on `pull_request`, where the matrix is deliberately path-narrowed
      and so is SUPPOSED to be a subset — asserting there would fail every PR and
      teach people to ignore the job.

      Verified: YAML parses; job graph is `lane → changes → platform →
      lane-coverage`; gating simulated for all four triggers (05:00 → lane and
      platform skipped, Zephyr jobs run; 07:00 and dispatch → full set;
      pull_request → lane-coverage off). Both shell blocks dry-run under bash
      against real `lane-coords` output: 8 lane modules all resolve to a home, and
      a simulated matrix that dropped freertos correctly reports the gap. The
      `cargo build -p nros-tests --bin lane-coords` step was verified to work in a
      cleaned environment with `FREERTOS_PORT` unset (80 s), and needs the nested
      resolver submodule, which the job now inits.

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
- [x] The codegen golden corpus is committed, and a deliberate template change
      fails **tier 1** with a readable diff. *(Both arms run 2026-07-31. The lane
      is real: `check-cli-tests` → `check-build` → `check` → `ci`, and it runs the
      whole `packages/cli` workspace, so `codegen_golden` is inside tier 1 rather
      than beside it. Adding `// PERTURBED` to one line of `_nros_field.jinja`
      failed with the changed line quoted golden-vs-now across three named files
      plus "… and 3 more"; restoring gave an empty `git diff` and 2/2 green.)*
- [x] With no Python available, `resolve_fp` falls back to the resolver's binary
      hash and the gate still refuses to call a stale fixture fresh. *(Three arms,
      2026-07-31, cache backed up and restored around them. Resolver present but
      unable to resolve → `binary:e042c59a…`; resolver absent → `resolver-absent`;
      normal → `26ac1134…`. All three differ, so a signature recorded under any one
      of them cannot match another — the degraded modes over-invalidate, and no
      path produces a value that would let a stale fixture verify fresh. The
      remaining asymmetry is benign: `resolver-absent` is a constant, so it cannot
      distinguish "no resolver now" from "no resolver then" — but the term is only
      included for records that declare a bringup, and those cannot be BUILT
      without a resolver, so that state is unreachable.)*

## Non-goals

- Reproducible Rust binaries (the fingerprint sidesteps the question).
- Replacing cargo/ninja fingerprinting for rust/cmake fixtures — those are exact
  and self-healing already.
- Removing `NROS_SKIP_FIXTURE_CHECK=1`. It stays; the aim is that it stops being
  routine.
