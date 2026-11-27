---
rfc: 0143
title: "Stack-floor walk precision: narrower indirect binding and an exact search"
status: Draft
area: tooling
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-592]
supersedes: []
superseded-by: null
---

# RFC-0143 — Stack-floor walk precision: narrower indirect binding and an exact search

**Status:** Draft. Four questions below wait on a ruling; no code before it.

## Summary

The post-link stack-floor walk (issue 2400, phase-557) answers "how deep can
this thread's stack get" from the frames the compiler measured. Two defects
keep that answer from meaning what it says:

- **It is order-dependent** (issue 2550). The walk memoises each function's
  depth in a call graph with one large cycle, so the result depends on the
  order callees are tried in. It reports the deepest of 8 fixed orders, which
  is a lower bound on the worst order, not an upper bound on the stack.
- **It is inflated** (issue 2658). Indirect calls bind to every function of a
  coarse type (`void(ptr)` in C; every implementation of a trait slot in
  Rust), which closes that cycle and puts paths no thread runs into the
  floor: 3-10x the measured peak on the images that query the graph or
  declare a parameter.

Both have one cause: the bound edges are too loose. This RFC decides how to
narrow them for Rust `dyn` calls and for C function pointers, and then how to
replace the 8-order heuristic with an exact search that either finishes or
refuses.

## Motivation / problem

Measured (issue 2550, 2658; phase-557 W7f, W9c):

- With direct edges only, the reachable strongly connected component (SCC)
  is 24 functions. With bound edges it is 962 (Rust talker), 970 (action
  server) and 2,037 (C talker). No single caller and neither language alone
  breaks it.
- An exact search over that SCC did not finish (494 M calls in 120 s).
- From 8 to 64 callee orders the max rose 1.7-6.1 % and had not converged.
- Removing every edge looser than a slot (the "bind ceiling") takes the
  Nucleo action server from 113,808 B to 50,984 B against a measured peak
  of 30,328 B, and realtime-c's OPEN role from 188,308 B to 32,824 B. What
  is left above the peak there is large direct frames, which the frame-size
  work owns, not this RFC.

So binding precision is where the floor goes, and an exact search is only
affordable once the SCC is small. Landed narrowings, kept as they are: the
discarded-code rule and typed C from cargo build scripts (2658, 2026-11-23),
which took realtime-c OPEN from 92,324 B to 76,148 B.

## Design

### D1 — Rust `dyn` calls bind to vtables the image constructs

A `dyn Trait` call loads a function from a vtable slot. Today it binds to
that slot in every implementation of the trait the graph holds. The image
can only call an implementation whose vtable it builds, and a vtable is a
read-only data symbol that code references where the trait object is made
(`&x as &dyn T`, `Box<dyn T>`).

Rule: a `dyn` site in slot S of trait T binds to slot S of each vtable for T
that some function the walk reaches references (a relocation from that
function's code, or from data that such code references). Vtables come from
the IR (`@vtable.*` globals with their slot functions) and their referrers
from the linked objects' relocations, the same inputs the walk already
reads. A vtable nothing live references contributes nothing.

### D2 — C function pointers bind through the field or parameter they travel in

Reopens phase-557 W7f's design, which stopped at its checkpoint because
under half of the sites bound through a trusted channel (37-43 % on Nucleo,
about 0 % on mps2 bare metal for lack of C IR). Two of its three
prerequisites have since landed (C IR for cc-rs builds through
`compile_commands.json`; the discarded-code rule). The rule:

- a site that calls a pointer loaded from struct type T, field F binds to the
  functions stored into (T, F), including through nested initialisers;
- a site that calls a parameter binds to what its direct callers pass, and
  a function returning a pointer counts as storing it (the return-value
  channel);
- a cast, `memcpy` or escape of the field keeps today's coarse binding for
  that site, so precision never costs soundness.

### D3 — An exact search under a node budget

Condense the graph into its SCCs. Outside an SCC the deepest path is exact by
dynamic programming over the DAG. Inside one, search simple paths, reusing a
depth only when it was computed with no cut to an ancestor still on the
stack (`scripts/lib/exact_walk.py`'s `deepest_simple`, today a measurement
only). The search spends at most a stated budget of visited nodes per root.

- Finished: the floor is the deepest simple path, independent of callee
  order. This replaces the 8-order heuristic and its `WALK_ORDERS`.
- Budget spent: see Q3.

True recursion that survives narrowing (a cycle of direct edges) is refused
naming its functions, never folded into a number.

### D4 — What a floor states

A finished walk's floor is "the deepest path through the edges bound",
which is an upper bound on the thread's stack exactly when the binding is
sound (D1/D2 keep a coarse binding wherever precision is not proven). The
docs and the report drop "heuristic floor, order-dependent" once D3 lands.

## Open questions (for the ruling)

- **Q1 — D1's reach.** Bind `dyn` sites by live vtables (as drafted), or by a
  full points-to analysis over MIR? Drafted: live vtables. It uses inputs
  the walk already has and needs no compiler plugin; points-to is far more
  work for a gain not yet measured.
- **Q2 — D2's order against D1.** Measure D1 alone first, on the two 2658
  images and realtime-c, and decide on D2 from that number? Drafted: yes.
  D2 was stopped once already; it reopens only if D1 leaves the SCC too
  large for D3.
- **Q3 — A spent budget.** When the exact search runs out: (a) refuse on an
  image with an `[[observed]]` row and warn on an unmeasured one, as issue
  2550's fix text says; or (b) fall back to the sound SCC bound (the sum of
  the SCC's frames), which is a true upper bound but often more than the
  board's RAM. Drafted: (a), with the budget and the SCC size reported, so a
  refusal says what to narrow.
- **Q4 — Acceptance.** Issue 2658 asks for a floor "within the lane's margin
  of the measured peak, or a stated reason why the gap is the price of
  soundness". The bind ceiling shows the Nucleo image cannot reach its peak
  by binding alone (50,984 B against 30,328 B, the rest is large direct
  frames). Drafted: acceptance is the walk finishing (D3) on every
  `[[observed]]` row, order-independent, with a floor at or above its peak;
  the gap to the peak is reported, and closing it is the frame-size work's.

## Not in this RFC

- Entity-kind pruning (issue 2517) stays parked under its own reopen
  conditions; D1-D3 do not need it.
- Shrinking large frames (`out_of_line`, `declare_parameter_overridden`,
  `Executor::pass`) is the frame-size batch's.
- Rooting each sched task at its own callbacks (from issue 2784, now in
  2658) follows D1-D3: it narrows nothing while every task shares the same
  loose edges.

## Changelog

- 2026-11-27: Draft, from the batch study of the stack-floor issues
  (2550, 2658, 2517, 2784, 2653). Four questions for a ruling.
