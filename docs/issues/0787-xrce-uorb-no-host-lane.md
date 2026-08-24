---
id: 787
title: "The xrce and uORB backends have no host lane, so their C sources are
  edited and reviewed but never COMPILED until tier 2"
status: open
type: tech-debt
area: ci, rmw, xrce, uorb
related: [phase-376, issue-0773, issue-0778, issue-0319, issue-0652]
---

## Problem

`just check-rmw-cyclonedds` builds the cyclonedds backend and runs its 17-test
suite on the host, on the fast line, with no SDK provisioning. There is no
equivalent for the other two C backends:

| backend | host lane | what compiles it |
| --- | --- | --- |
| cyclonedds | `just check-rmw-cyclonedds` | fast line, every run |
| xrce | **none** | tier 2 fixture builds only (needs the Micro-XRCE-DDS SDK) |
| uORB | **none** | tier 2 fixture builds only (needs PX4) |

So an edit to `packages/rmw/xrce/nros-rmw-xrce/src/*.c` or
`packages/rmw/uorb/nros-rmw-uorb/src/*.cpp` gets no compiler feedback at all
during normal development. It is reviewed by reading, and by whatever the C ABI's
own type checking catches when the VTABLE initialiser is compiled — which is
nothing, because the vtable lives in the same untouched-by-any-lane TU.

## Why it is worth an issue rather than a shrug

Phase-376 W5 changed these two backends five times:

* the `const` handle class (15 slots, then 3 more)
* the `void` → `rmw_ret_t` return class (6 slots)
* `create_*` taking `const rmw_node_t *`
* the length-or-status class (issue 0773)
* `send_request` / `take_response` carrying a sequence id (issue 0778)

Every one of those was a signature change across a C ABI seam. Cyclonedds
caught real mistakes in several of them — a `constexpr` function-pointer alias
whose type no longer matched, an internal helper left non-const, a caller
passing the wrong argument count. Those same mistakes in xrce or uORB would
have shipped to tier 2 undetected, and the only reason issue 0773's xrce half
was found at all is that a STRUCTURAL gate went looking for the shape rather
than a compiler finding it.

That is issue 0319's pattern (a gate nobody runs) and issue 0652's (a target no
lane builds), applied to a whole backend instead of a check or a test target.

## What a fix looks like

Both SDKs are provisionable — `nros setup` handles them, and tier 2 already
builds these backends, so nothing is missing except a lane that does it early.
Two shapes, either acceptable:

1. **A skipping lane, like cyclonedds'.** `check-rmw-xrce` / `check-rmw-uorb`
   that compile the backend when the SDK is present and print a `nros_check_skip`
   line when it is not. Costs nothing on a host without the SDK and gives every
   provisioned host and CI runner a compiler.
2. **A syntax-only lane with stub headers.** Cheaper and weaker: it would catch
   signature and arity errors — which is the entire class above — without
   needing either SDK. Worth considering if provisioning in CI is the blocker.

Whichever, the skip must be LOUD and counted (`scripts/build/check-skip.sh`),
so "no SDK here" never reads as "the backend compiles".

## Until then

Any commit touching those two backends should say in its message that they were
not compiled. Several phase-376 commits do; that is a convention, not a gate,
and this issue exists because a convention is not enough.
