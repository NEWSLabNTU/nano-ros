---
id: 2469
title: "live-peer's host lane fills the hosted disk mid-build: its intermediates release stops after the first root under mawk, and the workspace rows grew from 57 to 73"
status: open
type: bug
area: [ci, testing, fixtures]
severity: medium
found: 2026-11-12
related: [2130, 2318, 1846]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, the
home of scheduled lanes that end without a verdict. This one gave none in
three straight runs, so the fix #3528 made to the bridge cells (`case_01`,
`case_06`) is still unconfirmed.

## Measured

`live-peer`'s `rows whose board IS this runner` job, GitHub-hosted
`ubuntu-22.04`:

| run | outcome |
| --- | --- |
| 42957501616 (11-11 04:10) | 57 workspace rows built; reached the cells with 34 G free; 3 real failures (a verdict) |
| 42978245897 (11-11 07:16) | `No space left on device` during `examples/workspaces/bridge-cyclonedds` |
| 43153068357 (11-12 04:10) | 73 workspace rows; free space 106 G at 04:21, 7.0 G at 06:00:56; the log stops at 06:01:47 mid-build |

In 43153068357, `live-peer.yml` sets no `timeout-minutes`, and the job ended
at 2 h, far short of the hosted 360-minute default. It was not a timeout: the
runner died of a full disk. The per-row cost in that run was 28 G for the
first, cold row and about 2–3 G for each row after it.

## Two causes

**1. The release stopped after its first root.** Issue 2130's
`scripts/ci/release-fixture-intermediates.sh` counted bytes with
`du -cb | awk 'END {print $1+0}'`. The CI image's `awk` is mawk, which prints a
total past 2^31 in exponent form:

```
release-fixture-intermediates.sh: line 71: 1.74022e+10: syntax error: invalid arithmetic operator
  released 0 MiB of compile intermediates in total
```

(run 42957501616). The arithmetic error aborts the loop over roots. `build/`,
the first root, was released (free space rose from 20 G to 36 G), every
example build dir after it was not, and the report said 0. gawk prints the
integer, so this never failed on a developer host. Reproduced with
`awk` → `mawk`: same error, the second root untouched.

**2. The workspace loop grew past what one release at its end can absorb.**
Phase-531/535 made the Linux examples one-package workspaces, so the host
cells' derived workspace set went from 57 rows to 73 (the step's own comment
still said it "reaches 8"). The loop released once, after the last row, on
the grounds that "the rows share one cargo target per workspace". That holds
inside one workspace, but most of these rows are now their own workspace.

What a C/C++ example row keeps, measured on `examples/linux/c/talker`: 1.8 G
built, 1.2 G after the release. 755 MB of that 1.2 G is build-script
executables, the debuginfo-heavy host half of issue 2318.

## Fix

- `release-fixture-intermediates.sh` takes its byte counts from `du` as
  integers (`tail -n 1 | cut -f1`, non-digits refused). It now also releases
  a build script's own executable (`build/<crate>-<hash>/build{-,_}script{-,_}build*`).
  That executable is never run again, and the fixture probe reads only the
  script's `output` and `out/` (`find_build_script_outputs`), which stay.
  The talker row then keeps about 0.5 G instead of 1.2 G.
- A self-test on every run, on the normal path (`check-gate-selftests`'s
  rule), and the gate `release-fixture-intermediates` runs it: two roots, the
  first with a 3 GiB SPARSE library. Both are released, the total is an integer,
  binaries, build-script `output` and `out/` are kept, and build-script
  executables go. The old script fails the same case under mawk.
- `live-peer.yml` also releases inside the workspace loop whenever free space
  drops under 30 G. The cost is a rebuild of what a later row of the same
  workspace shares, paid only when needed.

Sweep: `git grep -n 'awk' -- '*.sh' '*.just' '.github/workflows/*.yml' | grep END`.
Of the `awk … END { print … }` sites, only this one feeds bash arithmetic.
`runner-sweep.sh` prints a field it keeps as text.

## Closes when

A scheduled `live-peer` host job reaches its cells (a verdict, pass or fail),
with the free-space trace showing the loop staying above its threshold.
