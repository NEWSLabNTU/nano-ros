---
id: 1491
title: "`just check test-targets` writes a 329 MB `target-excluded-tests/` at the
  repo root that `.gitignore` does not cover — and `disk-report.sh` says it does"
status: resolved
resolved_in: 2026-09-25
type: bug
area: build
related: [0400]
---

## Problem

`just check test-targets` runs `scripts/run-excluded-crate-tests.sh`, which sets

```sh
# scripts/run-excluded-crate-tests.sh:151
CARGO_TARGET_DIR="$(nros_scoped_target_dir excluded-tests)" \
```

`nros_scoped_target_dir <suffix>` (`scripts/build/cargo.sh`) resolves to
`${CARGO_TARGET_DIR:-$PWD/target}-<suffix>`, so on a plain host that is
`<checkout>/target-excluded-tests/`.

The root `.gitignore` enumerates every OTHER scoped dir and not this one:

```
/target-zpico-build-matrix/
/target-zpico-drift-gate/
/target-zenoh-fixture-posix/
/target-zpico-multisession/
/target-embedded/          <- nros_scoped_target_dir embedded
/target-param-services/    <- nros_scoped_target_dir param-services
```

There are exactly three `nros_scoped_target_dir` call sites outside the helper
(`embedded`, `param-services`, `excluded-tests`) and only `excluded-tests` is
missing. MEASURED on 2026-09-25 after one `just check test-targets` in a clean
agent worktree:

```
$ du -sh target-excluded-tests
329M    target-excluded-tests
$ git check-ignore -v target-excluded-tests
(no output — NOT ignored)
$ git status --short
?? target-excluded-tests/
```

## Why it matters

This is precisely the class CLAUDE.md's "Never `git add -A` / `git add .`" rule
exists for: a gate on the merge-gating PR lane leaves 329 MB of cargo output
that `git status` reports as untracked, at the repo root, next to six siblings
that are ignored. A blanket add commits it; a careful reader has to know which
of the seven `target-*` dirs is scratch.

## The doc already asserts the fix

`scripts/ci/disk-report.sh:82` reads:

> `target-excluded-tests`, plus four more the root `.gitignore` enumerates.

which names this dir as one of the enumerated ones. It is not. A comment
stating a fact that is one line away from being true is the shape that keeps a
gap open.

## Direction

Add `/target-excluded-tests/` to the root `.gitignore` beside its siblings.
Then consider whether the enumeration should exist at all: the list is
AUTHORED and its producer (`nros_scoped_target_dir`) is a single function, so a
gate could derive the required entries from the call sites rather than trusting
seven hand-written lines to keep up — the same "the map is authored, so it
drifts" shape as the rmw parity map. Whoever takes it should check
`disk-report.sh`'s comment in the same commit.

Found while measuring issue 1473 (the gate ran, the dir appeared); not caused
by that change and not fixed on its branch, to keep that one single-purpose.

## Resolution — 2026-09-25

Three parts, because the one-line fix was the smallest of them.

**The line.** `/target-excluded-tests/` now sits beside `/target-param-services/`
in the root `.gitignore`. Verified: `git check-ignore -v target-excluded-tests/`
answers `.gitignore:104`, and a directory planted there with a file in it is
invisible to `git status --short`.

**The comment that asserted it.** `scripts/ci/disk-report.sh` read
"`target-excluded-tests`, plus four more the root `.gitignore` enumerates",
naming this dir as already covered. It now says all seven are ignored and that
`target-excluded-tests` only became so here — a comment stating a fact one line
away from being true is what kept this open, and the issue said so itself.

**The drift, which is the part worth having.** The entries are HAND-AUTHORED, one
per suffix, and the producer is a single function — the issue's own "the map is
authored, so it drifts" observation, the rmw-parity-map shape. New gate
`check-scoped-target-dirs-ignored` (`just check scoped-target-dirs-ignored`,
fast lane, buildless) **derives** the required entries from the
`nros_scoped_target_dir <suffix>` call sites instead of trusting the list:

```
check-scoped-target-dirs-ignored: OK (4 call site(s) over 4 suffix(es);
every scoped target dir is ignored)
```

One direction only. A suffix a recipe asks for must be ignored; an ignore entry
that outlives its call site is not a defect, and a self-test case pins that.

**Negative control on the real tree:** removing the new `.gitignore` line makes
it red naming the exact site —
`scripts/run-excluded-crate-tests.sh:151: nros_scoped_target_dir excluded-tests
writes target-excluded-tests/ … does not enumerate /target-excluded-tests/`.
Restored, green.

Self-test 8/8 on the normal path, including the three readings that would make
it useless: a commented-out call site is not a call site, prose naming the
helper is not a call site, and a `#` inside a quoted string does not truncate
the line. Vacuity guard: fewer than three harvested call sites is a FAILURE,
since three recipes use one today — "OK (0 call sites)" is what a collapsed
scan prints.

Meta-gates after registration: `check-gate-lists` OK (357 fast gates),
`check-default-gates-run-somewhere` OK, `check-gate-selftests` OK.

**Not done:** the 329 MB directory measured in the issue is build output in one
agent worktree, not something this change deletes. It is ignored now, so it no
longer reads as untracked.

