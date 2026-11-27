---
id: 3013
title: "Discipline the image's wall clock by the SNTP server's measured rate, so a fast simulator's stamps stop saw-toothing between re-syncs"
status: open
type: enhancement
severity: low
area: [zephyr, platform, clock]
related: [rfc-0141, issue-3012]
found: 2026-11-27
reporter: "nros NXP 175 (ASI)"
---

## Deferred (user ruling, 2026-11-27)

Filed for a future RFC-0141 amendment. No work now.

## What

Between SNTP answers the image's wall clock is monotonic plus offset, so it
runs at guest rate. On an Arm FVP that races an idle guest, ASI measured the
stamp age at its control output saw-toothing between re-syncs: p50 about
-650 ms at a 2 s interval (ASI run 46018854324), because the guest clock runs
about 2x between answers. The re-sync already measures the server's rate
(RFC-0141 D5, issue 2884) but uses it only to pace queries.

The idea: scale the time since the last answer by that rate when reading the
wall clock, so the image's clock follows the server's between answers.

## Why it is deferred

RFC-0141's non-goals exclude continuous time discipline: the store holds one
offset, and the read path is `monotonic + offset`. Rate discipline changes
the read path and the store's model (offset plus rate plus anchor). It needs
its own amendment: when a rate is trusted (issue 3012's step rule), how
discipline interacts with `upgrade` and the stamp table, and what a port
without a re-sync does.
