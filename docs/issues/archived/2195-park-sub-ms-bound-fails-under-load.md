---
id: 2195
title: "`park_sub_millisecond_deadline_is_honoured_both_ways` fails under host load: its upper bound measures the scheduler, not the park"
status: resolved
type: bug
severity: low
area: [platform, testing]
related: [1242]
found: 2026-10-31
---

## What

`nros-platform-cffi`'s `c_port_posix_wake` test
`park_sub_millisecond_deadline_is_honoured_both_ways` (issue 1242's guard
against a millisecond-floored park) went red inside `just ci gate`'s
`check::build` (`required-features-tests` lane) on 2026-10-31:

```
no 300 us park finished inside a millisecond (fastest 3.879082ms) — the park is still floored at the millisecond ABI
```

The host's load average was 38 at the time. Rerun solo it passed 3 of 3, and
the diff under test was docs-only.

## Why

The upper bound was absolute: the fastest of 20 unsignalled 300 us parks must
finish inside 1 ms. Taking the fastest of several absorbs one late wake-up,
but under sustained load every wake-up is late by more than 700 us. The bound
then measures the host's scheduling latency and says nothing about the park.

## Resolution (2026-11-01)

Each park is now paired with a plain 300 us `thread::sleep` taken right before
it, which suffers the same wake-up latency. The bound passes when the fastest
park is inside 1 ms (as before) OR when the median per-pair difference (park
minus sleep) is under 350 us. The lower bound (never before the deadline) is
unchanged and still checked on every park.

Measured on this host, with 96 busy loops for the loaded runs (load average 35):

| park | quiet host | loaded host |
| --- | --- | --- |
| as shipped | PASS 3/3 | PASS 5/5 |
| mutated to round up to whole ms | FAIL 3/3 (median ~780 us) | FAIL 3/5 (median 700 us) |

So under load the bound can miss the regression, but it can no longer report
one that isn't there. A quiet run still catches it every time; the remaining
gap is written into the test's doc comment.
