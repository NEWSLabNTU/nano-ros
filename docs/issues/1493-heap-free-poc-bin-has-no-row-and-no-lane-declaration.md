---
id: 1493
title: "`fixture_source_coverage` has been red on main for 27 days over
  `heap-free-poc-mps2`, and the reason nobody saw it is that no merge-gating
  lane runs that test"
status: open
type: bug
area: ci, testing
severity: medium
related: [0540, 0538, 1040, 1158, 0952, phase-350, phase-391, phase-468]
found: 2026-09-25
---

# The red

On `main` at `3cb028263`, with nothing built and no fixture involved:

```
$ cargo nextest run -p nros-tests --test fixture_source_coverage
FAIL [0.012s] nros-tests::fixture_source_coverage every_test_bin_is_a_row_or_a_tracked_exception

thread '...' panicked at packages/testing/nros-tests/tests/fixture_source_coverage.rs:145:5:
phase-350 W6 test-bin coverage gate (35 crate(s) scanned, 1 tracked exception(s)):
1 test bin(s) with NO `dir =` row in examples/fixtures.toml and no tracked
exception. ... Add a row, or add a BINS_ALLOWLIST entry naming the lane that
builds it:
  - heap-free-poc-mps2
```

## The confounders were ruled out first

CLAUDE.md records four retracted ghost issues (0859-0862) filed from a sweep
whose artifacts predated the tree, so this was checked before it was believed:

* **Not a `skip!` counted as a failure.** The panic is the `assert!` at
  `fixture_source_coverage.rs:145`, with the gate's own message. A
  `nros_tests::skip!` would have carried `[SKIPPED]`.
* **Not staleness.** The test reads two things and builds nothing: the
  directory listing of `packages/testing/nros-tests/bins/` and the `dir =`
  values in `examples/fixtures.toml`. There is no fixture, no artifact and no
  mtime in its inputs.
* **Reproduces solo**, in 0.012 s, on a checkout at `origin/main` with no
  local edits.

## Which of the three explanations is true

Not "an orphan bin that compiles for nobody" — that was issue 0540's shape and
this is not it. **`heap-free-poc-mps2` is built by a merge-gating lane**, and
the chain is static and checkable:

| link | evidence |
| --- | --- |
| the build | `just/ci.just:196-201` — `cd packages/testing/nros-tests/bins/heap-free-poc-mps2 && cargo build`, then `scripts/check-no-alloc-image.py --tier heap-free` over the linked `thumbv7m-none-eabi` image |
| the recipe that holds it | `ci::l3` |
| its other spelling | `just/ci.just:332` — `_matrix-build: l3`, i.e. `just ci matrix build` IS `l3` |
| the workflow | `.github/workflows/queue.yml:170` runs `just ci matrix build` on `merge_group` |
| it really runs | `vars.NROS_SELF_HOSTED_READY` is `true` (set 2026-09-10), and the `L3 (cross build + link)` job reports `success` on recent queue runs (e.g. run 36072859171, 2026-09-24) |

So the gate is not over-reaching and its rule is not narrower than its subject.
The rule is **"a manifest row, or a tracked exception naming the lane that
builds it"**, and this bin qualifies for the second arm — the arm exists
precisely for an artifact a recipe produces outside the fixture coordinate
space. What is missing is the one-line declaration. `b3fb82d4a`
("feat(phase-391 W1/W4, #816, #843): the heap-free tier, on a real image, gated
in ci-l3") landed the bin, the recipe and the symbol checker together, and did
not land either half of the declaration the gate asks for.

A `fixtures.toml` row would be the wrong arm. The artifact is a cross-linked
ELF with no runtime, consumed by a Python symbol-table checker inside the lane
that builds it; it has no platform x lang x rmw x kind coordinate, no
`matrix::CELLS` cell wants one, and no test resolves it as a fixture. Giving it
a row would put it in `build-test-fixtures` for a consumer that does not exist
and duplicate the `l3` build.

## When the red started, measured

Replaying the gate's own predicate against three revisions — bins directory
listing, `dir =` values, and the `BINS_ALLOWLIST` as each commit spells them:

```
b3fb82d4a~1   crates: 27   UNCOVERED: []
b3fb82d4a     crates: 28   UNCOVERED: ['heap-free-poc-mps2']
origin/main   crates: 35   UNCOVERED: ['heap-free-poc-mps2']
```

Green on the parent, red on the commit, red ever since. `b3fb82d4a` is
2026-08-29 16:44 UTC; `origin/main` is 2026-09-24 23:27 UTC. **27 days.** Seven
further bins landed in that window and every one of them got a row, so this is
a single missed declaration and not a decayed convention.

# The larger finding: the test runs in no lane anyone runs before pushing

This is issue 1040's rule and issue 1226's shape — a gate that WORKS is not a
gate that RUNS — and it is the reason a correct, instant, fixture-free
assertion sat red for four weeks with a merge queue running the whole time.

**Where it does not run:**

* **`just ci gate` (`just ci l1`) — the lane CLAUDE.md tells every contributor
  to run before every push.** Its `test-unit` step is
  `cargo nextest run --workspace --exclude nros-tests` (`justfile:1016`), and
  `fixture_source_coverage` is a `nros-tests` integration test. The exclusion
  is deliberate and correct for what `test-unit` is; the consequence is that
  this gate is outside it.
* **`just check`** — it is not a `check-*` gate and appears in no gate registry.
* **`test-lane-contracts`**, the gate lane's other non-`check::` step, which
  names exactly three test targets (`lane_run_narrowing`,
  `matrix_fixture_coverage`, `lane_build_covers_run`).
* **Every merge-gating event.** `gate.yml` runs `test-unit` and
  `test-lane-contracts`; `queue.yml` runs those plus `ci matrix build`. Neither
  reaches `-p nros-tests` at large.

**Where it does run — and why that produced nothing:**

* `just test-integration` and `just test`, i.e. local developer tiers.
* `just ci tier1` / `just ci matrix` / `just ci full`, via `test-all`'s
  `--workspace`. The coordinate narrowing does not hide it: narrowing happens
  in the fixture resolver, and this test resolves no fixture.
* The only workflows that reach any of those are `host-tests.yml` (the
  `just ci tier1` job — `push` to main with paths, nightly cron, dispatch) and
  `run-matrix.yml` (`just ci matrix`, scheduled). **Both have been red
  continuously.** `host-tests.yml`: every one of the last 25 runs is `failure`
  or `cancelled`, the failures all at the `just ci tier1` step.
  `run-matrix.yml`: the last 8 runs are all `failure`, which is issue 1158's
  standing finding that the tier-2 lane reaches no cells. A lane that is
  uniformly red has no signal capacity, so even the post-merge placement could
  not have surfaced this.

**Why `check-default-gates-run-somewhere` did not catch it.** That gate is the
rule for exactly this, and its scope — by construction, and stated in its own
header — is the gates `just check` runs plus the steps of `just ci gate`.
Issue 1226 already widened it once from the first of those to both. An
assertion that lives in a `nros-tests` integration test is in neither set, so
this class of gate is invisible to the gate that exists to find invisible
gates. `fixture_source_coverage` is not alone there: it is one of a set of
repo-invariant assertions under `packages/testing/nros-tests/tests/` that need
no fixture, no SDK and no QEMU, run in milliseconds, and are reachable today
only through a lane that either builds fixtures first or has been red for
weeks.

# What this issue asks for

1. **Landed with this issue:** the `BINS_ALLOWLIST` entry for
   `heap-free-poc-mps2`, naming `just ci l3` / `just ci matrix build` and the
   `queue.yml` job that runs it, with the reason a row is the wrong arm. That
   clears the red.
2. **Open:** decide where the fixture-free repo-invariant tests in
   `packages/testing/nros-tests/tests/` belong in the lane ladder. The obvious
   candidate is a fourth step on `just ci gate` beside `test-lane-contracts` —
   a named `cargo nextest run -p nros-tests --test <names>` list, which
   `check-lane-contracts` can then police for fixture stamps the way it polices
   the rest of that lane. Cost is a few hundred milliseconds. Doing it by
   binary name rather than by crate matters: `-p nros-tests` at large pulls in
   every QEMU and interop target, which is why `test-unit` excludes the crate
   in the first place.
3. **Open, and the part that generalises:** extend the reach of
   `check-default-gates-run-somewhere` past `just check` names and `ci gate`
   steps, so that an assertion carrying a gate's rule is required to be in
   *some* lane whatever file it lives in. Until then, writing a gate as a test
   rather than as a `check-*` script is a way to opt out of the rule without
   anyone deciding to.

## What is not measured here

The exact step at which `host-tests.yml`'s `just ci tier1` fails. The logs for
the runs listed above are no longer retrievable (`gh run view --log` returns
`log not found`), so this issue asserts only the conclusions, which are
retrievable, and does not claim which step ate them. That question belongs with
the lane, not with this bin.
