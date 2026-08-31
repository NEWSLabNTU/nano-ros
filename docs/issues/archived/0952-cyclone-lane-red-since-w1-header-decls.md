---
id: 952
title: "phase-406 W1 never reached the C++ backends: cyclonedds got new
  definitions and stale declarations (silent, C++ overloads), uORB got neither"
status: resolved
type: bug
area: rmw, build
related: [phase-406, 0876]
---

## Problem

`just check rmw-cyclonedds` failed to compile from phase-406 W1 (`3084f7bd9`)
until this fix. The whole backend and all 22 of its tests were unbuildable.

```
vtable.cpp:385:1: error: invalid conversion from
  'rmw_ret_t (*)(const rmw_node_t*, const char*, const char*, const char*, uint32_t, ...)'
  to 'rmw_ret_t (*)(const rmw_node_t*, const rmw_message_type_support_t*, const char*, uint32_t, ...)'
```

## Why it was silent

W1 collapsed `type_name` + `type_hash` into one `rmw_message_type_support_t *`.
In `nros-rmw-cyclonedds` it updated the four DEFINITIONS (`publisher.cpp`,
`subscriber.cpp`, `service.cpp`) and left the four DECLARATIONS in
`internal.hpp` at the old shape.

In C this is a redeclaration conflict and the compiler says so at the
definition. **In C++ it is an OVERLOAD**: the header declares one function, the
.cpp defines a different one, both are well-formed, and nothing complains until
some third file — `vtable.cpp`, taking the address for the slot table — resolves
against the declared overload and finds the types do not match. The error
therefore surfaces far from the edit, names neither file W1 touched, and reads
as a vtable problem rather than a header problem.

Note the header was PARTIALLY migrated: `publisher_publish_raw` and
`subscription_take` already carried the span types from W2. So "the header was
forgotten" is not right either — it was updated for one work item and not the
other, which is harder to spot than an untouched file.

## Why nobody noticed for days

**The first draft of this issue said `just ci l1` does not build the C/C++
backends. That is FALSE, and the correction is the useful part.** `check::build`
lists them explicitly:

    c cpp rmw-cyclonedds rmw-xrce rmw-uorb cli-tests node-std-tests

and `l1` is `check cli-fresh check::fast check::build check::api-parity` +
`test-unit` + `test-lane-contracts`. So tier 1 DOES cover this backend, and the
tier is not at fault.

What is true is the ORDERING. `just` runs that list in sequence and stops at the
first failure, so `check::fast` — 146 gates — stands in front of `check::build`.
**Any one fast-lane red masks every backend build behind it**, and the mask is
silent: the run ends with `check-fast: 1 of 146 gate(s) FAILED`, which reads as
one small problem, not as "and the other 200-odd lanes did not run".

That is easy to hit precisely during an ABI break. `check-abi-bindings`
regenerates and then runs `git diff --exit-code`, so it is RED for as long as
regenerated bindings sit uncommitted — which is the normal working state while
migrating an ABI. Every l1 run in that window dies before the backend builds.

This issue does not establish which run, if any, was made during W1 — that is
not recoverable from the tree, and guessing would be the same mistake as the
first draft. What is established: the lane was red from `3084f7bd9`, tier 1
covers it, and tier 1 has a failure mode that hides it exactly when an ABI
change is in flight.

`rmw-xrce` is the control: it is in the same `check::build` list, its smoke test
broke the same way in this work, and tier 1 caught it — once the fast lane was
green enough to reach `check::build`.

## uORB had it too, and worse

Found when tier 1 finally reached `check::build` after the cyclonedds fix.
`nros-rmw-uorb` was not half-migrated — **W1 and W2 never touched it at all**.
Its declarations AND definitions were both still the pre-W1 shape, so the crate
itself compiled (both sides agreed) and only the vtable aggregate in
`vtable.cpp` failed, with the same conversion error cyclonedds gave.

That is the more dangerous version of this bug. Cyclonedds at least had a
disagreement inside the backend; uORB was internally consistent and wrong, which
no amount of reading one file can reveal. The only thing that catches it is
building the slot table — which is exactly the step behind the masking order
described above.

Both backends are now migrated: 6 uORB declarations, 6 definitions, 7 call sites
and one `uorb_test_take` adapter in `register_smoke.cpp`. `just check rmw-uorb`
passes 1/1.

## Fix

- The four cyclonedds `internal.hpp` declarations realigned with their
  definitions.
- `cyclone_get_node_names` migrated to `rmw_node_visitor_t` (the same W2
  grouping its sibling slots got).
- 33 call sites across 11 test files rewritten for the new create/publish/send
  shapes, plus three `nros_test_take*` adapters in `nros_test_domain.h` for the
  11 take-family calls, whose `out_len` is read inside an `if` condition.

Verified: `just check rmw-cyclonedds` builds and `100% tests passed out of 22`.

## The masking order — FIXED 2026-08-31, by the first option

`check::fast` before `check::build` is the right order for FEEDBACK and the
wrong one for COVERAGE: a one-gate red silently withdrew every expensive lane
behind it, and the run ended `1 of 149 gate(s) FAILED`, which reads as one small
problem rather than "and four steps never ran".

Of the two options, the first: **the lane now names what it withdrew.** Stopping
at the first failure is kept — running a ten-minute backend build after a
one-second gate has already gone red wastes the run you are about to redo
anyway. What was wrong was never the stopping; it was the silence.

`just ci l1` and `just ci full` now run their steps as a list and report:

    CI L1 FAILED at step 2 of 6.

      ok       check::cli-fresh
      FAILED   check::fast
      NOT RUN  check::build
      NOT RUN  check::api-parity
      NOT RUN  test-unit
      NOT RUN  test-lane-contracts

      4 step(s) did NOT run. This lane stops at the first failure, so a
      red step WITHDRAWS every step after it — `check::build` among them,
      which is the only place the C/C++ backends compile.

Verified by injecting a bogus `status` into `rmw-api-map.toml` and running the
lane, not by reading the code: the output above is that run.

`full` gets the same treatment, where it matters most — `just check` alone is
~200 gates in front of `test-all` and the Zephyr cell.

Still true, and still worth doing during an ABI break: run the backend lanes
explicitly rather than waiting for a green tier 1 to reach them. The difference
is that a red tier 1 no longer looks like it did.
