---
id: 952
title: "phase-406 W1 changed the cyclonedds DEFINITIONS and not the header
  DECLARATIONS: C++ overloading made it silent, and the lane was red for days"
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

`just ci l1` — the tier CLAUDE.md tells you to run before every push — does not
build this backend. Neither does `check-fast`. The cyclone lane is
`just check rmw-cyclonedds`, a cmake build nothing on the affordable tier
invokes. So every phase-406 commit ran a green tier 1 over a backend that had
not compiled since the first one.

This is the "a red lane answers one of two questions" entry in CLAUDE.md reached
from the other side: not a lane that is red and therefore carries no signal, but
a lane nobody RUNS and which therefore carries no signal either. The tier system
is a deliberate affordability trade, and this is its cost — worth stating plainly
rather than treating each instance as a surprise.

## Fix

- The four `internal.hpp` declarations realigned with their definitions.
- `cyclone_get_node_names` migrated to `rmw_node_visitor_t` (the same W2
  grouping its sibling slots got).
- 33 call sites across 11 test files rewritten for the new create/publish/send
  shapes, plus three `nros_test_take*` adapters in `nros_test_domain.h` for the
  11 take-family calls, whose `out_len` is read inside an `if` condition.

Verified: `just check rmw-cyclonedds` builds and `100% tests passed out of 22`.

## Not fixed here

Nothing gates "every backend lane compiles" at a tier anyone runs per task. A
`check-*` gate cannot fix it: the cost IS the build. The honest options are to
put the backend compiles on a nightly that is triaged (`just nightly-triage`
already classifies by step), or to accept that an ABI break must run the backend
lanes explicitly and say so in the phase doc. Phase-406's doc now says so.
