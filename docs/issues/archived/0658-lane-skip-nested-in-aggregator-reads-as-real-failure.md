---
id: 658
title: "A lane skip NESTED inside a matrix test's aggregate panic reads as a real failure: five tier-2 reds that are skips"
status: resolved
type: bug
area: testing
related: [issue-0445, issue-0584, issue-0482, issue-0599, phase-340, phase-354]
---

## Symptom

`just ci-matrix` (tier 2) reports 9 real failures. Five of them are lane SKIPS
that the junit rewriter did not convert:

| test | what it actually hit |
| --- | --- |
| `entry_e2e::entry_matrix` | `[SKIPPED:lane] … workspace-c-native-robot2 … linux,c,zenoh` |
| `multihost_e2e::multihost` | `[SKIPPED:lane] … workspace-c-native-robot1 …` |
| `realtime_tiers_e2e::realtime_tiers` | `[SKIPPED:lane] … workspace-c-native-realtime …` |
| `roundtrip_xprocess_e2e::roundtrip_xprocess` | `[SKIPPED:lane] … workspace-c-native-service-server …` |
| `sched_dims_applied_e2e::sched_dims_applied` | `[SKIPPED:lane] … workspace-rust-nuttx-realtime …` |

Each fails SOLO under the tier-2 lane, so this is not load.

**The partial case is the defect; the total case already works.** Unnarrowed
(tier 1) four of the five PASS, and `entry_matrix` — whose cells are ALL out of
tier 1's scope — skips cleanly and is rewritten to `<skipped>`. That is the same
test, taking a different branch:

| cells out of lane | branch taken | marker position | rewriter |
| --- | --- | --- | --- |
| all | `nros_tests::skip!` directly | starts the panic | converts ✅ |
| some | the aggregator's FAILED tally | nested line 3+ | misses ❌ |

So the machinery is right for the case it was built against and wrong the moment
a run is PARTIALLY in lane — which is what every tier below `ci-full` is.

## Two defects, one symptom

### 1. The aggregator counts a lane skip as a FAILED cell

`entry_matrix`'s own panic:

```
entry_matrix: 11 of 15 cell(s) FAILED (4 ran, 0 skipped):
  zephyr/c/entry_pubsub: [SKIPPED:lane] out of lane: workspace-c-native-robot2 is at
  coordinate linux,c,zenoh, which this run's lane does not select …
```

**`0 skipped` while eleven cells are lane skips.** The per-cell result carries
the `[SKIPPED:lane]` verdict and the aggregator files it under FAILED, so the
test panics where it should have skipped (or run its four in-lane cells and
passed).

There is a second, deeper disagreement behind it. `entry_matrix` filters cells
with `lane_scope::admits(c.platform)` — a PLATFORM predicate — and then resolves
each surviving cell's fixture, which is filtered by COORDINATE. A `zephyr/c`
cell passes the platform filter and then resolves a fixture at `linux,c,zenoh`
that the lane rejects. That is exactly the "build-set and run-set are ONE
predicate on one coordinate file" rule (#482 / phase-340 W3) with a second
predicate still standing beside it.

### 2. The rewriter cannot see a marker that is not at the start

`scripts/test/skip_marker.py` recognises a skip two ways, and a nested marker
defeats both:

```python
SKIP_AT_START_RE.match(text.lstrip())          # payload: marker must START the text
PANIC_SKIP_RE = r"panicked at [^\n]*\n\[SKIPPED(?::([a-z_]+))?\]"   # stream: marker on the line AFTER "panicked at"
```

Here line 2 is `entry_matrix: 11 of 15 cell(s) FAILED …` and the marker is on
line 3+, indented. Both anchors miss, so `rewrite-skipped-junit` leaves a
`<failure>` and `name-real-failures.py` reports a real red.

The anchoring is deliberate and correct on its own terms — its comment says "a
real failure may legitimately mention the word" — so the fix is not to relax the
regex to a substring search. That would make any test whose failure text quotes
a skip verdict silently vanish from the real-failure count, which is the
opposite defect and a worse one.

## Why it matters

This is [issue 0445](0445-staleness-verdict-absorbs-the-runtime-failure-behind-it.md)'s absorbing-skip class one layer out. There the
lesson was that a STALE verdict replaces whatever the fixture would have done;
here a LANE verdict is absorbed by an aggregator that then re-reports it as a
failure, and the rewriter — the mechanism that exists to keep skips from reading
as reds — is blind to it by construction.

Concretely it means **tier 2 cannot currently produce a clean verdict**: five of
its nine reds are skips, so anyone running the tier has to hand-triage them,
which is the state #0599 and #0584 were built to end.

## Fix sketch

Fix (1) and (2) is implied by it:

- The matrix aggregators must classify a per-cell `[SKIPPED:lane]` result as
  SKIPPED, not FAILED, and account for it in the `(N ran, M skipped)` tally.
  Then a run whose cells are all out of lane calls `nros_tests::skip!` itself and
  the marker lands at the START of the panic, where the existing anchors already
  find it. No regex change needed.
- The classification belongs in ONE place — the per-cell result type, next to
  `nros_tests::fixtures::lane` — not in five aggregators, or this recurs the
  next time a matrix test is written. Compare `exempt_probe_input`'s single
  spelling (`check-staleness-probe-exemptions`), which exists for the same
  reason.
- Separately: drop the platform-level `lane_scope::admits` pre-filter in favour
  of the coordinate predicate, or state why two predicates are correct here.

**Gate it.** The class is "a skip that reaches the harness wearing a failure's
clothes". A check that no `<failure>` payload CONTAINS a `[SKIPPED` marker
anywhere — distinct from the start-anchored classification, and reported as
"this should have been a skip" rather than silently rewritten — would catch all
five of these and the next aggregator too.

## Repro

```
just build-test-fixtures lane=tier2
source scripts/build/fixture-lane.sh
coords="$(nros_lane_coords_file tier2)"
NROS_FIXTURE_LANE=tier2 NROS_TEST_COORDS="$(realpath "$coords")" \
  cargo nextest run -p nros-tests -j1 -E 'test(entry_matrix)' --no-capture
```

Fails solo; passes with `NROS_TEST_COORDS` unset.

## Found by

phase-354 W3 (2026-08-17). The wave's own change is unrelated — this surfaced
while establishing a tier-2 baseline for it, and the five reds predate it.

## Resolved 2026-08-17

**Root cause, one line:** every one of the five aggregators tested
`msg.contains("[SKIPPED]")` — the BARE marker. `[SKIPPED:lane]` does not contain
that substring, so `skip_class!`'s output (issue 0584) fell into the `failed`
bucket. Not a subtle interaction: five independent copies of one wrong literal.

The same literal had ALREADY been fixed once, in the junit rewriter's
`_is_skipped_failure` (phase-340). It came back because there was no shared
helper to reach for — the "fix the CLASS, add ONE helper rather than a second
spelling" rule, learned again.

### What landed

| | |
| --- | --- |
| `nros_tests::skip_marker` | the one Rust spelling — `class_in` / `is_skip` / `starts_with_skip`, mirroring `scripts/test/skip_marker.py`. Five unit tests, including one asserting the pre-fix literal would have missed both a classed and a nested marker. |
| the five aggregators | now call `skip_marker::is_skip(&msg)`. |
| `check-skip-marker-matching` | in `check-fast`. Forbids a hand-rolled match against a `"[SKIPPED…"` literal outside the helper. Self-tests against the ACTUAL pre-fix line on every run, through the same predicate the scan uses. |
| `name-real-failures.py` | a failure whose payload contains a marker NOT at the start is still counted real — only the aggregator can know — but is now NAMED: `<- a skip marker is NESTED in this failure`. So the next occurrence announces itself instead of costing a hand-triage. |
| `skip_marker.PREFIX` | added, so Python has the constant Rust does. |

The start-anchored classification is unchanged, deliberately. Relaxing it to a
substring search would delete real failures that quote the word — the opposite
defect, and worse.

### Measured

Tier 2, same lane and fixtures, before and after:

| | reds | of which |
| --- | --- | --- |
| before | 9 | 5 nested lane skips, 1 load flake, 2 px4 STALE, 1 qemu |
| after the aggregator fix | 3 | 2 px4 STALE, 1 qemu — none carrying a nested marker |
| after the px4 fix | **2** | 1 qemu (`test_qemu_wcet_benchmark`), 1 load flake that passes solo |

Each of the five passes solo under the tier-2 lane (they RUN their in-lane cells;
`multihost` runs two).

### Left standing, deliberately

* **The platform-vs-coordinate double predicate.** `entry_matrix` still
  pre-filters on `lane_scope::admits(platform)` before resolving fixtures
  filtered by COORDINATE. With the classification fixed this is no longer a red,
  but it is still two predicates where #482 says one — worth folding in when
  someone touches these aggregators.
* ~~**The px4 pair.**~~ **Fixed in the same change.** `px4/rust/companion/*` has
  no `[[fixture]]` row — it is built by `just px4 build-fixtures`, its own lane
  behind its own SDK prerequisites — so `attribute_path` cannot place it and the
  "an unattributable path is never skipped" rule left it reporting STALE (a hard
  failure) under any narrowed lane.

  The rule is right; the outcome was not. `require_coord_in_lane` exists for
  precisely this shape — "a resolver that selected its row by configuration
  knows the coordinate outright" — so both px4 accessors now state the
  coordinate (`px4,rust,xrce`) through ONE `px4_companion_coord()` rather than
  relaxing attribution or inventing a manifest row for a lane that does not
  build from the manifest. `Rmw::coord_token()` names the manifest's spelling of
  the rmw field, delegating to `cmake_value()` so the shared vocabulary is
  stated in code instead of left as two tables that can drift.

  Verified in both directions: narrowed → `[SKIPPED:lane]`, unnarrowed → the
  genuine STALE hard failure is unchanged.
