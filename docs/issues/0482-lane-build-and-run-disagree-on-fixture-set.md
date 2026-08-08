---
id: 482
title: "tier 2 needs fixtures its own lane build does not produce — a row's coordinate is computed twice, and the lane's RUN is never narrowed"
status: open
type: bug
area: testing
related: [issue-0393, issue-0196, issue-0443, issue-0445, issue-0351, rfc-0061, phase-318, phase-337]
---

## Symptom (measured 2026-08-07)

After a clean, fully successful `just build-test-fixtures lane=tier2` — all 8
modules OK, stamp written, `_require-fixtures` satisfied — `just ci-matrix`
produced **~231 test failures**, nearly all of one shape:

```
Test fixture is STALE — a source is newer than the built binary:
  binary: packages/testing/nros-bench/stress-zenoh/target/nros-relwithdebinfo/zenoh-stress-test
  newer:  packages/api/nros/src/node.rs
  NOT RUN: 18th consecutive stale verdict for this fixture
```

Rebuilding the same tree with `lane=all` dropped it to **~19 real failures**.

So the tier-2 lane only passes on top of a tier-3 build. That is the exact
failure RFC-0061 exists to prevent: a middle rung whose real price is the top
rung's price is a rung nobody runs, and CLAUDE.md already names the consequence
— "an instruction nobody could afford per task, so it got followed selectively,
which is worse than a smaller instruction followed honestly".

Note the verdict is STALE and not "Binary not found" only because the tree had
older binaries lying around; issue 0445 applies (a STALE verdict is absorbing,
`18th consecutive` means the fixture never ran at all). On a fresh host the same
defect reads as "Binary not found".

## Two independent causes, both instances of "computed twice"

This is the issue-0196 / #393 family — *build-side selection and test-side
requirement must derive from ONE computation* — applied to lane membership.
There are two separate places where they do not.

### Cause A — a fixture row's COORDINATE is computed in two places, with two answers

A lane selects fixtures by `(platform, lang, rmw)`. `examples/fixtures.toml`
makes `rmw` OPTIONAL, and the two readers disagree about what an omitted `rmw`
means:

| reader | omitted `rmw` reads as | consequence |
| --- | --- | --- |
| `packages/testing/nros-tests/tests/matrix_fixture_coverage.rs` | `zenoh` (`get("rmw").unwrap_or("zenoh")`, commented "fixtures.toml convention") | the row is modeled, the coverage gate is green |
| `scripts/build/fixtures-manifest.py::matches_filters` | `None`, compared verbatim against the lane's triples | `(linux, rust, None)` matches **no** coordinate, so the row is in **no** lane |

**67 of 240 buildable `[[fixture]]` rows carry no `rmw`.** Every one of them is
invisible to every coordinate-scoped consumer — which is both the lane BUILD
(`fixtures-manifest.py --coords-from`) and the lane STALENESS GATE
(`check-fixtures-stale.sh`, same filter). So the gate could not report what the
build had skipped: the same blind spot hid both halves.

Measured over every computed lane (2026-08-07):

| lane | coords | `[[fixture]]` rows selected, as shipped | with the coordinate computed once | rows gained |
| --- | --- | --- | --- | --- |
| tier1 | 10 / 47 | 78 / 240 | 118 / 240 | 40 |
| tier2 | 13 / 47 | 46 / 240 | 109 / 240 | **63** |
| tier2-nightly | 35 / 47 | 127 / 240 | 193 / 240 | 66 |

`workspace_fixture` rows all carry an explicit `rmw` (`validate_workspace_fixture`
requires it) and are unaffected: 72 / 43 / 74 of 93 either way.

A round trip makes the same point without reference to any lane: hand the filter
**every** coordinate the manifest reports, and 67 of 240 rows still do not come
back, because `(platform, lang, None)` is a triple a `lane-coords` file cannot
spell.

The 63 rows tier 2 gains are not an exotic tail — they are essentially every
native Rust example and every bench, including the exact fixture in the symptom
above:

```
examples/native/rust/{talker,listener,service-*,action-*,logging,…}
packages/testing/nros-tests/bins/{entry-poc,qos-override-pubsub,contract-monitor,…}
packages/testing/nros-bench/{stress-zenoh,executor-fairness,stress-xrce,…}
examples/qemu-arm-baremetal/rust/*        (20 rows)
examples/qemu-esp32-baremetal/rust/*      (3 rows)
```

Tier 1 was immune in practice only by accident: its build lane is `native`,
which is a MODULE-level selection with no coordinate filter, so it builds every
native row whatever its coordinate. Tier 2 is the first lane to depend on the
coordinate filter for host rows, and it is the lane that broke.

**A sibling site, found by sweeping the class rather than the symptom.**
`matches_filters` compared the raw key for `--rmw` too, and
`just freertos build-fixtures` runs `fixtures-build.sh freertos rust zenoh`. So
the one `freertos, rust` row with no `rmw` —
`packages/testing/nros-tests/bins/logging-smoke-freertos-mps2` — was never built
by the command its own test names in its failure message ("*fixture not built —
run `just freertos build-fixtures`*"). 6 rows selected before, 7 after. Same
defect, different caller; routing `--rmw` through `row_coord` fixes both.

Sweep for the class:

```console
$ git grep -n 'get("rmw")\|\["rmw"\]\|entry.get("rmw")' -- scripts packages
$ git grep -n 'unwrap_or("zenoh")'
```

### Cause B — the lane's RUN is never narrowed, so BUILD ⊉ RUN

`ci-matrix` runs `test-all` with **no** `NROS_TEST_SCOPE`, i.e. the whole suite,
so every fixture of every coordinate must exist. But `_require-fixtures` is
handed `NROS_FIXTURE_LANE=tier2` and asks "does the stamp cover *tier 2*?" — to
which a `lane=tier2` build is a perfectly good answer. The preflight therefore
waves the run through and the run then discovers, one test at a time, that 34 of
47 coordinates were never built.

The justfile already *says* the right thing —

> Issue 0393 — this lane's BUILD is deliberately still `all`. […] Narrowing the
> build here would need the run narrowed to match first; until then, saying so
> beats a lane that silently under-building.

— but nothing ENFORCES it. `just build-test-fixtures lane=tier2` is an
advertised command, its stamp satisfies the preflight, and the contradiction
only surfaces as 231 red tests. A comment is not a gate.

Note this is the same shape as issue 0443, one level up. 0443 fixed *two
spellings of the lane* reaching two gates; this is *two different questions*
(what must be FRESH vs what must EXIST) being answered from one lane name as if
they were the same question. They are not: freshness is legitimately scoped to
the lane — that is tier 2's actual saving — while existence is scoped to the
RUN.

## Fix

**A — one coordinate per row.** `scripts/build/fixtures-manifest.py` becomes the
single computation of a row's `(platform, lang, rmw)` coordinate, applying the
documented `rmw` default in `row_coord()`; a new `coords` subcommand exposes it,
and `matrix_fixture_coverage.rs` consumes that instead of re-parsing the TOML
with its own default. There is then no second spelling to drift.

**B — the required BUILD derives from the lane's RUN.** `CiLane::run_scope()`
(the `NROS_TEST_SCOPE` a lane's recipe sets) and `CiLane::build_lane()` (the
fixture lane that covers that run) are declared once in `ci_lane.rs`, emitted by
`lane-coords --run-scope` / `--build-lane`, and consumed by
`nros_fixtures_stamp_require`. `ci-matrix` therefore fails at PREFLIGHT, in
seconds, naming `just build-test-fixtures` — instead of after 231 test failures.

## What this does NOT fix, and what would

Fix B makes tier 2 *honest*; it does not make it *cheap*. As long as
`ci-matrix` executes the whole suite, its true fixture cost is tier 3's, and the
only saving the lane delivers is a narrower freshness gate.

Making tier 2 cost what RFC-0061 claims requires narrowing the RUN, and the
selection must be DERIVED or it becomes the hand-maintained exclusion list
issue 0341 removed. Two candidate designs, neither small:

1. **Filter at selection time** (`scripts/test/lane-filter.sh`). Today it only
   knows `native` / `all`, and its tokens are platform families. Tier 2 selects
   1-wise over platform, so *every* platform appears in it and platform-level
   filtering excludes nothing — the saving is in lang × rmw *within* a platform,
   which test names do not encode reliably. Issue 0357 already established that
   binary-name filtering is insufficient for exactly this reason.
2. **Filter at resolution time** (the fixture resolver). The resolver is the one
   place where the test↔fixture mapping actually exists, so an out-of-lane
   coordinate could `skip!` rather than fail. But the resolver identifies
   fixtures by PATH across ~30 hand-written functions
   (`fixtures/binaries/mod.rs`), with no link back to the `fixtures.toml` row —
   the #328 shape. It would need the row identity threaded through first, and it
   risks laundering "never built" into "skipped", which is the 0445 hazard.

Both are phase-sized. Until one lands, `just ci-matrix` needs a full fixture
build and the ladder's cost table should say so.

## Reproduce

```console
$ just build-test-fixtures lane=tier2      # succeeds, stamp lane=tier2
$ just ci-matrix                           # ~231 STALE / not-found failures
$ just build-test-fixtures                 # lane=all
$ just ci-matrix                           # ~19 real failures
```

Cause A alone, without building a single fixture:

```console
$ cargo run -q -p nros-tests --bin lane-coords -- tier2 > tmp/t2
$ python3 scripts/build/fixtures-manifest.py list --lang rust --coords-from tmp/t2 | wc -l
10    # before — the 63 rows at (platform, lang, None) match nothing
73    # after
$ … | grep -c stress-zenoh
0     # before — the fixture in the symptom above
2     # after  (the plain row and its large-buffer variant)
```

## Gates added

* `just check-fixtures-manifest` now also runs `fixtures-manifest.py
  validate-fixtures` — plain `[[fixture]]` rows had **no** validator at all
  while the other two row kinds had one. A missing `platform`/`lang` (no
  coordinate) and an unknown `rmw` (a coordinate nothing holds) both fail there.
  The RMW-vocabulary check went into `validate_workspace_fixture` too; checking
  one of two coordinate-bearing tables is the issue-0196 shape.
* `packages/testing/nros-tests/tests/lane_build_covers_run.rs` — five
  assertions: the shell mapping equals `CiLane::run_scope`; an undeclared lane
  is refused rather than defaulted; a `lane=tier2` build does **not** satisfy the
  tier-2 run (the regression, and it fails on the shipped code); an `all` build
  satisfies every lane and a `native` build still satisfies tier 1 (so the fix
  cannot be "refuse everything"); and a coordinate-scoped build is refused for a
  module-level requirement *by failing rather than hanging* — without the guard,
  `comm -23 <(sort -u "$want_file")` gets an empty filename and `sort` reads
  stdin forever.
* Plus a round trip that fires on Cause A directly: hand the coordinate filter
  every coordinate the manifest reports, and every buildable row must come back.
  Pre-fix 67 do not.

Both gates were tripwired — reverted to the pre-fix behaviour, confirmed failing
(`67 row(s) UNREACHABLE`; the `lane=tier2` assertion flips to a pass), then
restored.

## Follow-up found while measuring (not fixed here)

Five rows relied on the implicit default while their leaf unambiguously builds a
DIFFERENT backend, so they are now modeled at the wrong coordinate and are
built/gated by the wrong lane:

| row | implicit coordinate | leaf actually uses |
| --- | --- | --- |
| `examples/native/rust/serial-talker` | `linux,rust,zenoh` | `nros-rmw-xrce-cffi` |
| `examples/native/rust/serial-listener` | `linux,rust,zenoh` | `nros-rmw-xrce-cffi` |
| `packages/testing/nros-bench/stress-xrce` | `linux,rust,zenoh` | `nros-rmw-xrce-cffi` |
| `packages/testing/nros-bench/large-msg-xrce` | `linux,rust,zenoh` | `nros-rmw-xrce-cffi` |
| `examples/qemu-arm-baremetal/rust/talker-xrce` | `qemu-arm-baremetal,rust,zenoh` | `rmw-xrce` feature |

Re-coordinating them is a behaviour change (it moves them between lanes, and
`qemu-arm-baremetal,rust,xrce` may not be a declared cell), so it is deliberately
NOT bundled with the drift fix. Adding an explicit `rmw` to these rows should be
its own change, run against `matrix_fixture_coverage`.
