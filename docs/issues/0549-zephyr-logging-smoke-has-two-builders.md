---
id: 549
title: "The Zephyr logging-smoke image has TWO builders writing two build dirs, and the manifest's one is unreachable"
status: open
type: tech-debt
area: build, testing
related: [issue-0535, issue-0539, phase-350]
---

## Problem

One fixture, two definitions, two output directories — and the one in the
manifest is built by no lane and read by nothing.

| | the REAL builder | the MANIFEST leaf |
| --- | --- | --- |
| declared in | `just/zephyr-dev.just:165` (`build-logging-smoke`) | `examples/fixtures.toml`, `builder = "west"` |
| writes | `<workspace>/build-logging-smoke/` | `<workspace>/logging-smoke-zephyr-native-sim/` |
| invoked by | `just/zephyr-ci.just:367` | **nothing** (see below) |
| read by | `binaries/mod.rs:3729`, via `logging_smoke.rs` | **nothing** |
| on disk here | 2.9 GB, current | absent — never built |

Both build the same source, `packages/testing/nros-tests/bins/logging-smoke-zephyr-native-sim`.

## Why the manifest leaf is unreachable

`zephyr-fixture-leaves.sh` gates that leaf behind `--include-logging-smoke`, and
**no build lane passes the flag**. Compare its sibling:

```
$ git grep -ln 'include-workspace-entry' -- just scripts
just/zephyr-ci.just                     <- the real lane passes it
scripts/build/zephyr-fixture-leaves.sh
scripts/check-zephyr-fixture-rows.py

$ git grep -ln 'include-logging-smoke' -- just scripts
scripts/build/fixture-inventory.py      <- read-only diagnostic
scripts/build/zephyr-fixture-leaves.sh
scripts/check-zephyr-fixture-rows.py    <- a gate
```

So the leaf is emitted for inventory and gating only. `just zephyr
build-fixtures` never produces it.

## This explains the `west_bare` anomaly

phase-350 W1 preserved a peculiarity of that leaf byte-for-byte rather than
"fixing" it mid-refactor, and declared it on the row as `west_bare = true`: it
emits **no cmake defs and an EMPTY staleness signature** into a real sig-file
path. An empty sig is a constant, so the leaf can never read stale, and it gets
none of the codegen-tool / toolchain-cache / sccache defs its siblings get.

That looked arbitrary. It is not: the leaf is a **vestigial second definition**
of a fixture the `zephyr-dev.just` recipe actually owns. Nothing consumes it, so
nothing ever noticed that its defs and signature were empty. The anomaly and the
duplication are the same fact.

## Why it matters

This is issue 0535's class — two spellings of one fixture — surviving *inside*
the manifest after phase-350 put everything else in it. It is currently benign
(the unreachable half costs nothing because it never runs), but:

* `just zephyr build-fixtures --include-logging-smoke`, or any future lane that
  passes the flag, silently starts building a second 2.9 GB tree that no test
  reads;
* the row claims a coordinate (`zephyr, rust, zenoh`) it does not actually
  occupy, so a coordinate-scoped lane counts it as covered;
* the real builder has no row, so it is invisible to `row_coord()`, to lane
  narrowing, and to the staleness gate — exactly what phase-350 W1 removed for
  the other 74.

## Direction

Pick ONE builder and delete the other; do not keep both behind a flag.

The `zephyr-dev.just` recipe is the one with consumers, so the cheap fix is to
point the manifest row at the dir that recipe writes (`west_build_name =
"build-logging-smoke"`), drop `west_bare` once the leaf gets the same defs and
signature as its siblings, and retire the `--include-logging-smoke` flag so the
leaf builds with the rest of the lane.

That is a behaviour change — it makes a currently-unbuilt leaf build — so it
wants its own measurement rather than riding along with a refactor. Verify with
`just zephyr build-fixtures` that exactly one logging-smoke tree exists
afterwards, and that `logging_smoke.rs` still resolves it.

Found while deleting dead build dirs after phase-350: `build-logging-smoke`
looked orphaned by every check that scanned manifest leaves and `-d build-…`
recipe arguments, because this recipe sets its build dir in a shell variable.
It came within one check of being deleted as garbage.
