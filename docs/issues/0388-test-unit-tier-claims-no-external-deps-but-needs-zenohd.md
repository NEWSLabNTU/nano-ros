---
id: 388
title: "`just test-unit` is documented as needing no external deps, but two of its tests require a built `zenohd` — and their skips are reported as failures"
status: open
type: bug
area: testing
related: [issue-0357, rfc-0051, rfc-0061]
---

# `test-unit` needs zenohd, and its skips read as failures

## Two defects, same run

Running the tier on a freshly provisioned host (SDK store populated via
`nros setup native --rmw zenoh`, no other setup):

```
Summary [7.458s] 817 tests run: 815 passed, 2 failed, 2 skipped
   FAIL nros-rmw-zenoh::status_events_matrix zenoh_event_matrix
   FAIL nros-rmw-zenoh::zenoh_integration two_sessions_deliver_cross_session_through_router
```

**D1 — the tier's contract is wrong.** `tests/README.md` and the recipe comment
describe `just test-unit` as "unit tests only (no external deps)", yet two of its
tests need a **zenohd binary at `build/zenohd/zenohd`**:

```
[zenoh-matrix] zenohd binary missing at …/build/zenohd/zenohd; tests will skip
panicked at …/tests/status_events_matrix.rs:133:
  zenoh client-mode session unavailable — is build/zenohd/zenohd built? Run `just setup`
```

Note the store is NOT the same thing: `nros setup … --rmw zenoh` installs zenohd
to `~/.nros/sdk/zenohd/<version>/bin/`, while the harness reads the repo-local
`build/zenohd/zenohd` (the `build/<tool>` convention, deliberately off PATH).
A host can therefore have zenohd installed and still fail this tier. `just test`
and `just test-all` declare and provision it (`build-zenohd`); `test-unit`
neither declares nor provisions it.

So either those two tests belong in `just test` (the tier that admits external
deps), or `test-unit` should depend on `build-zenohd` and the "no external deps"
description should go. The first is truer to RFC-0061's tier ladder: tier 1 is
meant to be the one anybody can afford, and source-building zenohd is not that.

**D2 — a skip is reported as a failure.** The second failure is not a failure:

```
panicked at …/tests/zenoh_integration.rs:242:
  [SKIPPED] second session refused — shim built with ZPICO_MAX_SESSIONS=1;
  rebuild with ZPICO_MAX_SESSIONS=2 to exercise multi-session
```

That is `nros_tests::skip!`, which panics by design; only `just test-all`'s junit
rewrite turns those panics back into skips. `test-unit` runs bare
`cargo nextest`, so every skipped precondition in the tier shows up red. CLAUDE.md
documents the trap ("Bare `cargo nextest` counts `nros_tests::skip!` panics as
FAILURES") — but documenting it does not stop the tier from lying about its
result, and tier 1 is the one people run most.

## Impact

The tier that CLAUDE.md tells every contributor to run after every change cannot
go green on a correctly provisioned host, and the red it prints does not
distinguish "you are missing a binary" from "your change broke something". On
this host that cost a full detour before the messages, read carefully, turned out
to name their own remedies.

## Direction

1. Decide the tier membership: move the two zenohd-dependent tests to `just test`
   (preferred — keeps tier 1 dependency-free), or make `test-unit` depend on
   `build-zenohd` and drop the "no external deps" claim from `tests/README.md`
   and the recipe comment.
2. Give `test-unit` the same junit rewrite `test-all` has, so a
   `nros_tests::skip!` is reported as a skip in every tier rather than only in
   the one that happens to post-process. A skip that reads as a failure trains
   people to ignore red, which is the expensive failure mode.
3. While there: `zenoh_event_matrix` prints `tests will skip` on stderr and then
   PANICS instead of skipping — pick one.

## D2 fixed (2026-08-02, `e6ed5d68a`)

`test-unit` now applies the same handling `test-all` and `_nextest-platform`
already had: capture rc, `_rewrite-skipped-junit`, pass iff
`_count-real-failures` is 0. The issue-#29 guard came with it — a nextest exit
that is not 100, or a missing junit, is still a build/setup failure, so a crate
that fails to compile cannot tally as "0 real failures" and green the lane.

Verified both directions in the distrobox, which has a real `[SKIPPED]`
(ZPICO_MAX_SESSIONS=1, issue 0389): the skip run rewrote 1 failure to
`<skipped>` and exited 0; an injected `assert!(false)` in a zpico-alloc unit test
gave `ERROR: 1 real (non-[SKIPPED]) test failure(s)` and exit 1.

**D1 remains open** — the two tests needing `build/zenohd/zenohd` while the
recipe advertises "no external deps". That is a tier-ladder decision (move them
to `just test`, or make `test-unit` depend on `build-zenohd` and drop the
claim), not a mechanical fix.

## Evidence

Arch Linux host + Ubuntu 22.04 distrobox, checkout `1d192d4f2`, box store at
`$NROS_HOME=~/.nros-box`. After `just zenohd setup` populated
`build/zenohd/zenohd`, `zenoh_event_matrix` passed and the tier went to
`816 passed, 2 skipped` with only the D2 skip-as-failure remaining.
