---
id: 685
title: "RETIRED as a duplicate — `NROS_ZEPHYR_RUNNER_RECORD` splitting the sizes-probe key was found and better fixed in issue 0446"
status: resolved
type: performance
area: build
related: [issue-0446, issue-0528, issue-0562, phase-343, phase-353]
---

## Symptom

`build/sizes-probe` measured **207 GB across 911 sub-keys**, at an 87.3x
duplication factor for `nros-core` (1310 rlibs, 15 distinct `-C metadata`
identities). phase-353 W4 had left it at **8 sub-keys / 2.2 GB**, with "a second
run of the same lane now creates ZERO new keys".

## Two separate facts, which the headline number hides

**1. Most of it is pre-W4 residue nobody cleared.** Sub-key creation by date:

```
  6  2026-08-06        356  2026-08-15
  2  2026-08-10        275  2026-08-16
130  2026-08-13         26  2026-08-17
 90  2026-08-14         19  2026-08-18
```

W4 landed on 08-15. The rate collapses immediately after, so the fix works —
what it did not do is delete the ~850 keys it had just made unreachable.

**2. One knob of the same class escaped it**, which is what the 26/19-a-day tail
is. Using W4's own `nros-probe-key-inputs.txt` provenance records: of 69 recorded
knobs, 7 vary across keys, and one dominates.

| varying knob | distinct values |
| --- | --- |
| **`NROS_ZEPHYR_RUNNER_RECORD`** | **329** |
| `NROS_ZEPHYR_TOOL_PATH` | 7 |
| `NROS_FIXTURE_COORDS` | 4 |
| `NROS_FIXTURE_SCOPE` | 3 |
| `NROS_CARGO_PROFILE`, `NROS_EXECUTOR_MAX_CBS`, `NROS_FIXTURE_LANE` | 2 each |

```
build/zephyr-fixture-make-driver/records/20260815-114550-116255-20266/build-c-action-…
                                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^ timestamp + pid
```

Timestamp and pid, i.e. precisely the shape W4 excluded for
`NROS_BUILD_LOG_DIR` — "differs every run, so every fixture build minted probe
keys that could never be reused". It names a record file the driver WRITES; an
output cannot be an input to a probed size.

## Why W4 missed it

W4's census ran a NATIVE lane. `NROS_ZEPHYR_RUNNER_RECORD` is set only by the
zephyr fixture driver, so it never appeared in the sample that produced the
exclusion list. The mechanism was right and the survey was narrower than the
population — the issue-0196 shape, in a measurement rather than a gate.

## Fix

Added to `KNOBS_THAT_CANNOT_CHANGE_A_SIZE` with its argument, per that list's
contract (one stated reason per entry). Issue 0528's invariant is untouched: a
knob that CAN change a probed size still splits the key, unknown knobs still
split by default, and the four content-bearing names (`NROS_BOARD_TOML`,
`NROS_PLATFORMS_DIR`, `NROS_MODEL_DIR`, `NROS_HOME`) remain unexcludable and
asserted.

`NROS_FIXTURE_COORDS` also varies and also carries a `mktemp` path, but it is
NOT excluded here: it names a file whose CONTENT selects coordinates, so the
per-knob correctness argument has to be made on its own terms rather than by
analogy. Left for whoever makes it.

## Also done

The ~200 GB of pre-W4 residue was deleted. It is a build cache; a wipe costs a
rebuild of the probes and nothing else.

## Provenance

Found 2026-08-19 while re-measuring issue 0446's census on a current tree — the
`sizes-probe` population had gone 25.8x -> 87.3x since 08-15, which is the
opposite of what W4's landing should have produced, so the discrepancy was worth
explaining rather than filing as growth.

## RETIRED 2026-08-19 — duplicate, and the other fix is the right one

A concurrent session found the same splitter and recorded it inside
[issue 0446](../0446-build-artifact-reuse-factors.md) rather than as its own
issue. Same knob, same mechanism, same day. Their fix is better and is what
landed.

**They established the fact I missed: `NROS_ZEPHYR_RUNNER_RECORD` has no
readers.** `zephyr-fixture-make-driver.sh` set it AND passed the identical path
as the runner's positional argument, and `zephyr-fixture-run-one.sh` reads only
`${1:-}`. Tree-wide, zero readers.

So the correct repair is to DELETE the dead export, which they did. Mine added
it to `KNOBS_THAT_CANNOT_CHANGE_A_SIZE` — in their words, "bookkeeping for a
variable with no consumer", and a permanent entry describing something that
should not exist. I reverted my entry when the merge surfaced theirs.

Worth keeping the distinction, because it generalises: an unread `NROS_*` export
is not free. `knob_identity()` sweeps every `NROS_*` deliberately — that
conservative default is what issue 0528 requires — so a dead export becomes a
directory-per-run. The lesson is "delete the variable", not "teach the key about
it".

The measurement in this issue stands and is not disputed: 911 sub-keys / 207 GB,
with creation collapsing after W4 landed, and the ~200 GB of pre-W4 residue
deleted here. Their census (209 sub-keys / 61 GB) was taken after that deletion,
which is why the two differ.
