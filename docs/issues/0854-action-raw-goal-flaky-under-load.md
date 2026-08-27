---
id: 854
title: "`action_raw_goal_ships_one_cdr_header` times out in-sweep and passes solo
  with a 16x margin — starved, not slow"
status: open
type: bug
area: testing
related: [phase-395]
---

## Problem

`action_raw_goal_ships_one_cdr_header` hits its 60 s nextest timeout when it runs
as part of a full sweep, and completes in **3.6 s** when run alone — 5/5 solo
passes.

It is the first entry in `.config/flake-quarantine.toml` (phase-395 W5), so it
still runs and still records, but no longer blocks.

## Why this is starvation rather than a slow test

The assertion is that a raw goal ships exactly one CDR header. That property does
not depend on timing. What depends on timing is *reaching* the assertion inside
the budget while ~31 other gates compete for the host.

A test that needs 3.6 s and is given 60 s is not close to its budget. A 16×
margin does not erode into a timeout because the code got slower; it erodes
because the process did not get scheduled. That is the distinction the quarantine
evidence rule exists to force: "it failed once" is indistinguishable from a real
intermittent defect, and quarantining a real defect is how one ships.

## What has NOT been established

- **Which resource is contended.** CPU is the obvious guess and is not evidence.
  The sweep also contends on the loopback network, on the router, and on the
  fixture target dir. Nothing here distinguishes them.
- **Whether it is the only test in this shape.** A 16× margin collapsing under
  load suggests the budget model is wrong for a whole class, not for one test.
  `nextest-slow-tests.py` reports durations; nobody has compared in-sweep against
  solo across the suite, which is the measurement that would answer it.

## Why the quarantine expires 2026-10-27

Quarantine without expiry is deletion with extra steps. Two months is long enough
that the answer can come from the contention measurement above rather than from
guessing, and short enough that the entry cannot be forgotten.

## Not to do

Do not raise the timeout to make it green. That converts a diagnosable
starvation signal into a longer sweep that still fails under heavier load, and it
destroys the 16× margin that is currently the evidence for the diagnosis.
