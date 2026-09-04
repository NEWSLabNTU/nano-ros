---
id: 1045
title: "Two unswept corners of fixture resolution: the Zephyr and ThreadX
  locators, and a staleness probe that announces its own degradation only on
  the STALE path"
status: open
type: bug
area: testing, build
severity: medium
found: 2026-09-04
related: [issue-1005, issue-1027, issue-0196, issue-0442]
---

## Why this exists

Issues 1005 and 1027 are resolved and archived. Each recorded a corner it did
not reach, and both are about the same machinery — how a test finds a fixture
and decides whether it is fresh — so they are collected here rather than lost
in two archived files.

## What is left

### The Zephyr and ThreadX locators were never swept (from 1027)

1027 fixed five sites across `binaries/nuttx.rs` and `binaries/freertos.rs`,
moving them off a leaf `target/` literal and onto the manifest row
(`groups::select_sole_row` → `groups::row_resolved_dir` for the artifact root,
`row_profile_dir` for the profile). Zephyr and ThreadX resolve differently —
west leaves, `librustapp.d` — and were explicitly out of that sweep.

Being different is not being correct: the class 1027 measured is "the build side
moved to the phase-340 shared group dir and the test-side locator did not", and
nothing has checked whether these two locators are on the right side of it. The
symptom when they are not is the one 1027 measured: a freshly built image
reported `not prebuilt`, with the fallback arm — whose job is to warn about a
real miscompile — firing for a reason that has nothing to do with it.

### A degraded staleness probe is invisible on the FRESH path (from 1005)

`staleness::probe_accounting()` carries the "INPUT SET UNMEASURED" announcement
and is rendered only inside a STALE message. On the FRESH path — the direction
that matters, because that is where a probe that examined nothing reads as a
pass — it says nothing at all.

That asymmetry is what let 1005's symlink defect run unnoticed across the entire
cross-compiled half of the tree: `zpico_recorded_inputs` returned **0 entries**
for every FreeRTOS / NuttX / ThreadX fixture, so the probe silently ran the
hand-authored bootstrap walk its own doc comment calls unreachable, and every
verdict it produced was FRESH.

Same shape as issue 0442/0445: a verdict that explains itself is worth more than
a verdict, and the explanation has to reach the path where the answer is "no
problem here".

## Acceptance

* The Zephyr and ThreadX locators are checked against the same predicate the
  other four now use — attributable to a manifest row, or a stated reason why
  that family cannot be.
* A probe running a degraded input set says so on the FRESH path too, so
  "examined 0 inputs" and "examined 2286 inputs" do not read identically.
