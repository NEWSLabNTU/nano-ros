---
id: 158
title: "NuttX realtime tier e2e prove high>low by a timing heuristic — robust to the observed flake, but not deterministic"
status: open
type: tech-debt
area: testing
related: [phase-281, issue-0149]
---

## Summary

`realtime_tiers_{rust,c,cpp}_nuttx_e2e` prove the two tiers are live + correctly
ordered (10 ms `ctrl` faster than 100 ms `telem`) by **counting received
samples**: anchor on the slow tier reaching 5, then require the fast tier to
reach 15 (≈3×) within a window. This is a heuristic on wall-clock delivery, not
a deterministic proof of per-tier scheduling.

The `>1`→`>15` margin fixed the observed flake (grabbing `ctrl_out` at the first
sample let `ctrl_n` tie/fall below `telem_n` → `5≤5`, de-flaked in `00d8b8719`),
and the 3× margin is comfortable for a 10× rate ratio even with zenoh delivery
batching. But under **extreme scheduler jitter** (an overloaded CI host, a
stalled QEMU icount tick, a slirp hiccup) the fast tier could still fail to reach
15 in the window, or the counts could compress, producing a spurious failure.

## Direction (if it ever flakes again)

Replace the count heuristic with a **deterministic** per-tier signal: have each
tier's node emit a monotonic sequence number + a tier tag (or a coarse
timestamp) in its published payload / log line, and assert the *ratio of sequence
progress* over a fixed observation window, or simply that both tiers advance
their own counters. That proves each tier's timer fired at its own period without
depending on cross-tier wall-clock racing.

Low priority — the current margin has not flaked since `00d8b8719`; file so a
future flake has a home + a fix direction rather than a silent retry bump.
