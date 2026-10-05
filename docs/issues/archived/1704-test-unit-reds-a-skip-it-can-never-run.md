---
id: 1704
title: "`test-unit` printed `1 failed` on every run — a multi-session test whose
  precondition is a BUILD input the lane never provides, raised as a runtime
  `skip!`"
status: resolved
resolved_in: 2026-10-06
type: bug
area: [ci, testing, rmw-zenoh]
severity: medium
related: [0388, 0389, 0393, 0695, 1161, 1377]
found: 2026-10-06
---

## Symptom

Every local `just ci gate` and every merge-queue `test-unit` printed one red:

```
        FAIL [   0.240s] (1635/1814) nros-rmw-zenoh::zenoh_integration two_sessions_deliver_cross_session_through_router
     Summary [   6.997s] 1814 tests run: 1813 passed, 1 failed, 5 skipped
rewrite-skipped-junit: rewrote 1 [SKIPPED] failure(s) to <skipped> in target/nextest/default/junit.xml
All failures were [SKIPPED] preconditions — treating as pass.
```

(merge-group `gate` run 37346833722, 2026-10-05; the same on this host.)

## What it was — measured

A SKIP, not a defect. Measured on a clean `origin/main` worktree:

```
thread 'two_sessions_deliver_cross_session_through_router' panicked at
  packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs:264:13:
[SKIPPED] second session refused — shim built with ZPICO_MAX_SESSIONS=1;
  rebuild with ZPICO_MAX_SESSIONS=2 to exercise multi-session
```

The router started; the first session opened; the second was refused because
the shim's session pool is 1, the shipped default. `nros_tests::skip!` panics,
nextest has no runtime skip, so nextest's own summary says `1 failed`; the junit
rewrite then demotes it and the lane exits 0. The reason was even declared in
`.config/capability-skip-baseline.txt`, so `check-skip-budget` accepted it.

So the lane was not blind — a second, real failure would still have failed it —
but the line a human reads said `1 failed` on every run, forever. CLAUDE.md's
rule: a lane that always shows one red cannot visibly show a new one, and every
reader had to re-learn that this one did not count.

## Why it could only ever skip there

`ZPICO_MAX_SESSIONS` is a BUILD input (`zpico.c`'s pool, the Rust shim's
session-indexed tables). The default 1 is right for every shipped target and
must not be raised tree-wide (issue 0393). The capability has its own lane,
`just test-zpico-multisession`, with `ZPICO_MAX_SESSIONS=2` in its own target
dir. In every other lane the test was selected, built against a pool of 1, and
could do nothing but skip — a precondition no host can supply, because it is a
property of the build the lane chose.

## Fix

The requirement is a compile-time fact, so it is expressed at compile time:

* `nros-zpico-build`'s runner publishes the RESOLVED pool size on zpico-sys's
  `links = "zpico"` channel (`cargo:max_sessions=N`). One resolution, default
  included — no second read of the env var with a second copy of its default.
* `nros-rmw-zenoh/build.rs` reads `DEP_ZPICO_MAX_SESSIONS` and sets the
  `zpico_multi_session` cfg when it is >= 2 (declared via `rustc-check-cfg`).
* The test is `#[cfg_attr(not(zpico_multi_session), ignore = "…")]`. In a
  single-session build it is a NATIVE skip (nextest's `skipped` count, not
  `failed`). Where it is not ignored the second open is an ASSERTION — a
  refusal there is exactly the defect the test exists to catch.
* The baseline entry `second session refused — shim built with
  ZPICO_MAX_SESSIONS=1` is retired (the ratchet shrinks).

## Verification

* Default build, the test selected alone: `0 tests run: 0 passed, 20 skipped`,
  nextest exit 4 `no tests to run` — which is also the NEGATIVE CONTROL for the
  multi-session lane: had its env failed to reach the shim, its positional
  `two_sessions` filter would select nothing and `_nextest-tolerant` reports a
  nextest exit other than 100 as an ERROR, never a pass.
* `just test-zpico-multisession`: `1 test run: 1 passed` (plus `loan_e2e` 2/2).
* The edge is incremental in ONE target dir: `ZPICO_MAX_SESSIONS=2` then unset
  flipped the cfg both ways with no clean (cargo re-runs a dependent build
  script when its `links` dependency's metadata changes).
* `just ci gate`: the `test-unit` summary no longer carries a `failed` count.

## Class sweep

`two_sessions_deliver_cross_session_through_router` was the only `[SKIPPED]` in
`test-unit` (merge-group run: 1 failed of 1814, 5 ignored). The sibling
build-input test `loan_e2e` already asserted `ZPICO_MAX_SESSIONS >= 2` and lives
only in the multi-session lane; the peer-mode tests assert against
`ZPICO_PEER_MODE_SUPPORTED` (issue 0682) — both already own their verdict.
