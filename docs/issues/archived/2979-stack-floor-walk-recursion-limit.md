---
id: 2979
title: "The stack-floor walk recursed once per call-graph level, so a deep image failed post-link with RecursionError, and passed on a retry"
status: resolved
resolved: 2026-11-26
resolved_in: "the change that adds this issue"
type: bug
severity: high
area: [tooling, stack-floor]
related: [issue-2400, issue-2414, issue-2550]
found: 2026-11-26
---

Reported by the ASI migration session (S32Z builds blocked).

## What happened

On `s32z270-rtu0-zephyr` (the ASI project at nano-ros pin `1a638e9958`),
every fresh build, 3 of 3, failed in the post-link stack floor check
(`scripts/nros-stack-floor.py`, run by `zephyr_final`'s post-build
`check.cmake`) with a Python `RecursionError` inside
`worst_path` / `visit`. A second `cmake --build` of the same build
directory passed.

## Cause

`worst_path` is a depth-first search over the image's call graph, and it
recursed once per level. CPython stops at about 1000 nested calls, and this
image's graph is deeper than that.

Why the retry passed: the walk runs eight walk orders. When a jobserver
grants tokens (`nros build`, west under the token pool) they run in
fork-pool worker processes; with none (a bare `cmake --build`) they run one
after another in the walker's own process. A worker's Python stack starts
deeper. Measured on the old walk, a chain of 989 functions walks serially
and only 976 in a worker. So a graph near the limit failed in the first
build and passed in the second: the verdict depended on the host's
parallelism, not on the image.

Confirmed by the reporter's fresh reproduction at the same pin:

- build 1, fresh: `nros build`, then `west build -p auto` under nros's
  jobserver. It printed `nros-stack-floor: 8 walk order(s) from `main` on
  8 job token(s)`, raised `RecursionError` in `visit` inside the pool's
  `map`, and exited 1;
- build 2, retry: `cmake --build` of the same directory, plain ninja with
  no jobserver. It printed `on 1 job token(s) (no jobserver: serial)`,
  walked in full (no `answered from the walk cache` line, and a fresh walk
  cache directory stayed empty), and exited 0.

The self-test below is the regression: the deep graph, and the walk in a
fork-pool worker equal to the serial walk.

No floor that passed was wrong: the serial retry's answer is the complete
walk.

## Fix

The search is a generator per function that yields the callee it needs,
driven from an explicit stack: the same order, memo, on-stack and cycle
bookkeeping, and no Python recursion, so it answers the same in a worker
and in the walker's own process.

`nros-stack-floor.py --self-test` gains:

- a 5000-deep synthetic call graph in which every link also calls back to
  the root, so cycle detection runs at depth (the old walk raises
  `RecursionError`; the new one walks it to 40016 B);
- the same walk inside a fork-pool worker, which must equal the serial
  answer.
