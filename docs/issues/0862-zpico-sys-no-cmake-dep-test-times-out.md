---
id: 862
title: "`zpico_sys_has_no_cmake_dep` times out at 60 s instead of answering a
  static question"
status: open
type: bug
area: testing, rmw
related: [phase-391]
---

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
