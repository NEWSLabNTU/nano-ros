---
id: 787
title: "The xrce and uORB backends have no host lane, so their C sources are
  edited and reviewed but never COMPILED until tier 2"
status: resolved
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

## Resolved 2026-08-25 — and the SDKs were never the blocker

`just check-rmw-xrce` and `just check-rmw-uorb` build each backend and run its
CTest suite. Both are on the `check-build` line beside `check-rmw-cyclonedds`.

The issue above assumed provisioning was the obstacle and offered "a cheaper
syntax-only lane with stub headers" as a fallback. Neither backend needed it:

* **xrce** vendors micro-XRCE-DDS-Client and micro-CDR as submodules, so its C
  compiles on any host with a C compiler. `cargo check -p nros-rmw-xrce-cffi`
  had been compiling it all along — nothing was wired to notice.
* **uORB** defaults `NROS_RMW_UORB_LINK_PX4=OFF` *precisely* so it builds
  without the PX4 SDK. The CMakeLists says so in a comment dated to its
  original phase.

So nobody had written the recipe. That is a smaller finding than "we need SDKs
in CI" and a more embarrassing one.

## What the lanes found the moment they ran

**uORB did not compile.** Phase-376 W5/B1 gave the four `create_*` slots a
`const rmw_node_t *`; the sweep's regex matched signatures with a NAMED
parameter, and uORB's two service stubs write `rmw_session_t* /*session*/` with
the name commented out. The signature was left alone and the body-insertion
still fired, so both functions referenced an undeclared `node`. Four
compile errors sitting in a committed, pushed tree.

**uORB did not LINK, and had not for much longer.** `register_smoke.cpp` stubs
`nros_rmw_cffi_register`, the legacy single-argument form. `vtable.cpp` calls
`nros_rmw_cffi_register_named`, which is phase 104.B.2 — so this test had been
unlinkable since the named registry landed, long before phase 376.

**uORB's smoke test was never registered with CTest.** The executable was
built; `enable_testing()` and `add_test` were absent, so `ctest` in that tree
printed "No tests were found!!!" A lane that only ran ctest would have gone
green over nothing — issue 0652's shape one more time.

**xrce did not link either**, for three overlapping reasons: the same missing
`_named` registry stub, and the platform clock/sleep/UDP primitives that live
in the Rust platform layer and have no provider in a standalone C build. Its
smoke test now stubs them, all failing — a stub that pretended to succeed would
make the test assert against a socket that does not exist. One genuine
phase-376 fallout too: `send_request` gained its `sequence_id` out-parameter
(issue 0778) and the call site still passed three arguments.

Every one of those is the predicted class. The backends were not "probably
fine, just unverified"; they were broken, and two of them had been broken for
longer than this campaign.

## Also caught while wiring this

`third-party/dds/cyclonedds` was BEHIND its recorded pin — the pull that
brought in the Zephyr atomics fix advanced the pointer and the local checkout
never followed. So every cyclonedds build earlier in this session compiled the
older submodule. Re-checked on the correct pin afterwards: 18/18.

