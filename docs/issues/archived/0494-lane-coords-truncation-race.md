---
id: 494
title: "`lane-coords-<lane>.txt` is truncated while cargo compiles, so a concurrent reader sees zero coordinates and every narrowed test fails"
status: resolved
resolved_in: phase-340
type: bug
area: testing
related: [issue-0482, phase-340]
---

## Symptom

`just ci-matrix` is NON-DETERMINISTIC. Same tree, same commit, two consecutive
runs:

| run | real failures (junit) | of which `no coordinates` |
| --- | --- | --- |
| first | **223** | **203** |
| immediate re-run | **20** | 5 |

The failure message is

```
<repo>/target/nextest/lane-coords-tier2.txt: no coordinates — refusing
```

## Cause

`nros_lane_coords_file` wrote the file with

```sh
cargo run -q -p nros-tests --bin lane-coords -- "$lane" > "$out"
```

The shell truncates `$out` the instant the redirection is set up. `cargo run`
then COMPILES — seconds to minutes — before writing a single byte. Every reader
in that window sees a zero-length file.

**The blast radius comes from a correct decision.** Issue 0482's narrowing fails
CLOSED on empty coordinates: an empty file must not be read as "no narrowing",
because that would silently run every test against a narrow build. So one
truncated file fails every narrowed test at once — 203 in the measured run —
rather than degrading quietly. The fail-closed behaviour is right; it is the
truncation that is wrong.

Note the file is NOT empty afterwards. Inspecting it post-run shows 13
coordinates, which is why this reads as "impossible" until the timing is
considered.

## Fix

Write to `"$out".tmp.$$` and `mv -f` into place. `rename(2)` within one
directory is atomic, so a concurrent reader sees either the previous content or
the new content — never a truncated one.

## Why this matters beyond the failures

A gate that reports 223 failures on one run and 20 on the next teaches people to
re-run rather than read, which is exactly how a real regression gets waved
through. The determinism is the point, not the failure count.

## Reproduce

Delete the file and start `ci-matrix`; any test that resolves coordinates during
the `lane-coords` compile window fails. Easier to observe by watching
`wc -c target/nextest/lane-coords-tier2.txt` during a cold run.
