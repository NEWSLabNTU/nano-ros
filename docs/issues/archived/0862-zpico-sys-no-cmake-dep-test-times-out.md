---
id: 862
title: "`zpico_sys_has_no_cmake_dep` times out at 60 s instead of answering a
  static question"
status: resolved
resolution: retracted
type: bug
area: testing, rmw
related: [phase-391]
---

> **RETRACTED — not a defect.** The original report is kept below
> unaltered, because a retraction that deletes its own evidence cannot
> be checked.

## Retracted 2026-08-28 — the run that produced this was invalid

This was filed from a `just ci-matrix` run whose FIXTURES PREDATED THE TREE.
The sweep built its artifacts at 03:19, the tree was then rebased past 04:48,
and the old results were read as current. Re-run against fixtures matching the
tree, it passes.

That is not a flake and not a partial fix: nothing about the reported behaviour
was real. See the retraction note at the bottom for the shared cause, which took
all four issues from that run.

## Re-check

```
$ cargo nextest run -E 'test(zpico_sys_has_no_cmake_dep)'
    PASS [   0.082s] (1/1) nros-tests::zpico_build_matrix zpico_sys_has_no_cmake_dep
     Summary [   0.092s] 1 test run: 1 passed
```

**0.082 s solo, against a 60 s timeout in the sweep.** The issue's own step 1
said to retest solo before treating the timeout as deterministic; doing that
answers it. The machine was running a 32-way fixture build plus other agents'
work at the time.

## What I got wrong in the original analysis

The core argument was wrong, and confidently so. I reasoned that "a build-graph
question answered in 60 s is doing something other than reading the graph", and
concluded the test probably compiles at run time — a "No compilation inside
tests" violation worth fixing on its own. It answers in 82 ms. It reads the
graph exactly as intended; it was starved, not misdesigned.

A timeout under load is evidence about the MACHINE, not about the code. Reading
it as a design defect produced a specific, false, and plausible-sounding claim.

---

# Original report (retracted, kept for the record)
## Symptom

`nros-tests::zpico_build_matrix zpico_sys_has_no_cmake_dep` TIMEOUTs at exactly
60.002 s on the tier-2 lane. It does not fail an assertion — it never finishes.

## Why the timeout is itself the finding

The test asserts a BUILD-GRAPH property: that `zpico-sys` carries no dependency
on cmake. A question about the shape of a dependency graph should be answered
by reading the graph, in well under a second. A 60 s wall means it is doing
something else entirely — most likely invoking a build — and that has two
separate problems:

* it makes the answer a function of machine load, so it will flake rather than
  fail, and
* compiling inside a test is against the repo's own rule (CLAUDE.md "No
  compilation inside tests"): compile in the build stage, assert the artifact.

So this is worth fixing as a test-design defect even if `zpico-sys` turns out
to be perfectly clean. A timeout tells you nothing about the property.

## Next measurement

1. Run it solo — a full-sweep timeout can be contention, and QEMU/build lanes
   flake under load, so a solo red is the first thing to establish.
2. If it still times out solo, find what it shells out to. If it builds, the
   fix is to move that to a build-stage fixture and assert the result.
3. Only then, whether the underlying no-cmake-dep property actually holds.

## Repro

    source ./activate.sh
    cargo nextest run -E 'test(zpico_sys_has_no_cmake_dep)'

## Provenance

Found by the first full tier-2 run in some time (2026-08-28); pre-existing on
main and unrelated to the work landing alongside it. Not yet retested solo — do
that before treating the timeout as deterministic.
