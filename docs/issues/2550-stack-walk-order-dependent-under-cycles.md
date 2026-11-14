---
id: 2550
title: "The stack walk's depth depends on edge order once the call graph has a large cycle, so the floor is a heuristic, not an upper bound"
status: open
type: bug
severity: medium
area: [memory, tooling]
related: [issue-2402, issue-2516, issue-2517, issue-2521, issue-2400, phase-557]
found: 2026-11-14
---

## What

`worst_path` in `scripts/nros-stack-floor.py` is a depth-first search that
caches each function's depth. In a graph with cycles, that cache can be
wrong:

- a function first reached while one of its ancestors is on the stack
  caches a depth that leaves out every path through that ancestor (the
  back edge counts the recursion allowance, 0);
- a later route that reaches the same function WITHOUT that ancestor on the
  stack reuses the cached depth, and misses those paths.

So the answer depends on the order callees are tried in. It is a heuristic,
not the deepest simple path, and not an upper bound.

This was latent while call graphs were nearly acyclic. `--indirect bind`
(issue 2402) follows every indirect call to every function it can reach, and
callback edges close one large cycle.

## Measured (2026-11-14, nucleo-f767zi `nros new` templates, bind walk)

The strongly connected component (SCC) the root reaches:

| image | functions reached | largest SCC | frames in it |
| --- | ---: | ---: | ---: |
| Rust talker | 2,006 | 962 | 168,968 B |
| Rust action server | 2,019 | 970 | 173,224 B |
| C talker | 3,756 | 2,037 | 309,728 B |

The same graph, with only the callee order changed (sorted, then four
seeded shuffles):

| image | depth range |
| --- | --- |
| Rust talker | 58,860 – 68,080 |
| Rust action server | 58,988 – 68,024 |
| C talker | 76,364 – 86,220 |

The sorted order gave the C talker 84,524 B. A shuffled order found a path of
86,220 B through the same graph, so the reported depth was BELOW a path the
walker itself finds.

An exact search that never reuses a depth computed with an ancestor on the
stack did not finish on the Rust talker (494 M calls in 120 s). The sound
SCC bound, the sum of the SCC's frames, is 170 KB for the Rust talker and
311 KB for the C talker, more than the board's 384 KB RAM can give a stack.

In practice every observed peak sits 2–7x below the walk (issue 2400 W5b,
phase-557 W7). The floors are on the safe side, but the stated invariant
("the walk covers every path") does not hold.

## Interim (ruled 2026-11-14)

The recorded floors stay, and nothing is held. Docs say "heuristic floor,
order-dependent pending exact search", never "upper bound".

The walk is deterministic. The reported depth is the deepest over a fixed
set of N = 8 callee orders (`WALK_ORDERS = 0..7`):

- order 0 sorts callees by name;
- order k > 0 sorts them by `sha256("<k>:<name>")`.

The name is taken WITHOUT Rust crate hashes (`Cs…_`, legacy `17h…E`). Crate
hashes change with the project's path, and a path-dependent order would
bring back issue 2521's nondeterministic refusal.

A self-test pins the four-node case where the sorted order under-states:
it reads 112 B, against a true depth of 121 B.

## Fix

1. **An order-independent search under a node budget.** Reuse a cached depth
   only when it was computed with no cut to an ancestor still on the stack.
   Past the budget, warn on an unmeasured image and refuse on a measured
   one.
2. **Shrink the SCC so the exact search fits.**
   - Call-site types for C (issue 2516).
   - Pruning entity kinds an image does not declare (issue 2517).
   - Finding the `from_c` edges that close the cycle.
3. **Refuse true recursion that survives the shrinking.** It is never folded
   into a number.

## Acceptance

The walk reports the deepest simple path from its root, or it refuses.
That answer does not depend on callee order.
