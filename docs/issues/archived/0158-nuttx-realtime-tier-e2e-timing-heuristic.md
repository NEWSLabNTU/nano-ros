---
id: 158
title: "NuttX realtime tier e2e prove high>low by a timing heuristic — robust to the observed flake, but not deterministic"
status: resolved
type: tech-debt
area: testing
related: [phase-281, issue-0149]
resolved_in: (this commit)
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

## Resolution (2026-07-08)

Replaced the sample-COUNT heuristic with the deterministic per-tier proof the
issue's Direction called for. Each tier publishes a MONOTONIC counter, so the
highest delivered value (`nros_tests::max_int_after(out, "Received:")`, a new
shared helper) = how many times that tier's OWN timer fired — independent of how
many individual samples zenoh delivered (batching/drops distort the count, not
the max). The assertion is now `ctrl_max >= 3 * telem_max` (10 ms vs 100 ms ⇒
~10×). Also dropped the `wait_for_output_count(ctrl, 15)` gate that could TIME
OUT under jitter (the flake surface): after the slow-tier anchor the guest is
stopped and both observers are drained, then compared by max value.

Applied to all three `realtime_tiers_{rust,c,cpp}_nuttx_e2e` (the issue's named
tests) and the native `realtime_tiers_e2e` (identical count heuristic). The
counter is 0-indexed (5 samples ⇒ max 4), so the low-tier check asserts
*advancement* (`telem_max > 0`), not a count floor. Verified: the native lane
passes 3/3 with the new assertion; the three nuttx lanes compile-check + share
the identical logic (their runtime is the CI nuttx lane).
