---
id: 1451
title: "phase-451's first acceptance criterion had no check: nothing asked whether a
  cmake module defining a public `nano_ros_*` verb was reachable"
status: resolved
type: tech-debt
area: [build, tooling]
found: 2026-09-22
related: [1218, phase-451]
---

# The rule was written down and never enforced

[phase-451](../roadmap/phase-451-dead-build-declarations.md)'s acceptance reads:

> `grep`-reachable: no cmake module defining a public `nano_ros_*` verb is
> unreachable from any `include()`.

W1 deleted the module that prompted it — `packages/api/nros-c/cmake/NanoRosLink.cmake`,
which defined `nano_ros_link_rmw`, was included by nothing, and **misled two of
four independent readers in one session** because it held a fourth closed RMW
list and a force-link that does not happen. It could not have worked either:
the `find_package` targets it named were deleted years apart.

Nothing was added to ask the question. The phase's own W1 note explains the
cost, and is itself the evidence:

> **Landed TWICE on the same day, independently.** […] Neither session knew of
> the other. […] nothing in this tree asks whether a declaration is REACHABLE,
> so the same dead one can be found twice in one day by two people looking for
> different things.

## What the check does

`scripts/check-cmake-verb-reachable.py`, on the fast line. Every tracked
`*.cmake` defining `function(nano_ros_*` or `macro(nano_ros_*` must be named
from somewhere else in the tree — by module basename or by one of its verbs.
Internal spellings (`_nano_ros_*`, `_nros_*`) are out of scope: they carry no
promise to a reader.

Measured on the live tree: **10 modules define a public verb, every one
reachable.** Discovery failing is a FAILURE, not a pass — a run that finds no
modules at all reports that and exits non-zero, because this tree has many.

## It exists because the hand version got it wrong

Asked by hand while auditing phase-451, this reported a second dead module,
`cmake/NanoRosProviders.cmake` (public verb `nano_ros_load_providers`). It is
not dead: `packages/testing/nros-tests/tests/provider_index_gate.sh` includes
the module and calls the verb. The hand check missed it because its output was
piped through `head -6` and the first six matches were all the module's own
lines — the truncation was read as the answer.

The deletion was already in progress and had to be reverted. Two consequences
went into the gate:

* it searches **every tracked file**, not just `*.cmake` and `CMakeLists.txt`,
  which is what the hand grep had restricted itself to;
* a **test's** include counts as reachability. A module exercised only by its
  own test is reachable; whether that is reason enough to keep it is a
  judgement for a reader, and the gate's job is to make sure the reader is
  looking at a true list rather than a truncated one.

Both directions are controls: green on the live tree, and red on a planted
module with a public verb nobody names.

## Still open in phase-451

The W4 box says the host lane's exclusions are "a hand-written string" needing
derivation. That is stale — `HOST_UNCHECKABLE` is
`` `bash scripts/build/embedded-only-members.sh` ``, derived from
`[package.metadata.nros] embedded-only = true`. What remains of that box is the
other half: `nros-platform-stm32f4` is in the derived set, so its three
`detect_phy_type` tests still run nowhere, and reaching them needs a per-crate
test lane rather than workspace membership.
