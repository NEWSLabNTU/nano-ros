---
id: 1491
title: "`just check test-targets` writes a 329 MB `target-excluded-tests/` at the
  repo root that `.gitignore` does not cover — and `disk-report.sh` says it does"
status: open
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
