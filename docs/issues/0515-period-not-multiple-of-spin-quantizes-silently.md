---
id: 515
title: "A timer period that is not a multiple of its tier's spin period quantizes to the spin grid — deterministic jitter, predictable at resolve time, reported nowhere"
status: open
type: enhancement
area: orchestration
related: [issue-0505]
---

## Problem

A tier's executor only evaluates timers when it spins, so a timer fires
on the first spin boundary at or after its period elapses. When the
declared period is not an integer multiple of the tier's
`spin_period_us`, every activation lands on the grid instead of on the
declared cadence.

Measured on the FreeRTOS mps2-an385 lane, idle, guest-clock timestamps
(`nano-ros-rt-eval`, a 33 ms timer in a tier with `spin_period_us =
5000`):

```
steer: mean 33.001 ms  modes: 35 ms x475, 30 ms x303, 34 ms x80
```

The long-run mean is right — the sub-period remainder carries over, so
the rate is preserved and no activation is dropped — but no individual
period is ever the declared one. The timer alternates between one spin
early and one spin late, a deterministic ±2 ms swing that a user
reading "33 ms" has no reason to expect. On the same lane, timers whose
periods ARE multiples of their tier's spin (10/50/100 ms on 1/10/10 ms
spins) sit on their nominal to within measurement noise.

Nothing reports this. It is not a deadline miss (the callback runs), not
an overrun (nothing is dropped, issue #505), and not a rate violation
(the mean is correct), so every runtime rule is correctly silent. The
jitter is invisible until someone measures cadence on target and tries
to explain a bimodal period distribution.

## Why it matters

The whole point of declaring timing at launch level is that the
declaration and the realization can be compared. This is a case where
they differ by a bounded, fully predictable amount, and the toolchain
that has both numbers in hand at resolve time says nothing.

For a control loop the practical consequence is a ±(spin/2) sampling
jitter that has to be budgeted somewhere; for anyone debugging on
target it is a puzzle whose cause is in a different file from the
symptom.

## Fix direction

This wants a resolve-time diagnostic, not a runtime rule — the
information is fully available before the image is built:

- Where the tier table and the entity periods are both known (the
  resolver / codegen path that already emits the per-tier scheduling
  parameters), warn when `period_us % spin_period_us != 0`, naming the
  entity, the declared period, the tier's spin period, and the two
  values the period will actually alternate between.
- Prefer a warning to an error: the behavior is legal and sometimes
  fine (a telemetry publisher does not care), and hard-failing would
  break existing workspaces on an aesthetic point.
- Worth considering as a follow-up: have the resolver suggest the
  nearest spin period that divides the requested cadence, since the
  spin period is usually the more arbitrary of the two numbers.
