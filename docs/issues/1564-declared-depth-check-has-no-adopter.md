---
id: 1564
title: "The declared-depth `static_assert` is opt-in and NOTHING has opted in — the only `qos: depth:` in the tree is in the one language the check cannot reach"
status: open
type: tech-debt
area: [build, core, testing]
severity: low
found: 2026-09-29
related: [1555, 1340, 1522, rfc-0100]
---

## What

`NROS_ASSERT_DECLARED_DEPTH` fails the BUILD when a subscription's passed QoS
depth disagrees with the depth its system declared for that topic in a contract
sidecar. The machinery is real and it works — `just check c` / `just check cpp`
compile `_Static_assert` TUs that prove a mismatch is a compile error.

**No shipping image exercises it.** Measured 2026-09-29:

| | |
| --- | --- |
| example contract sidecars (`git ls-files 'examples/*.contract.yaml'`) | 7 |
| of those, declaring a `qos: depth:` | **1** — `examples/native/rust/listener/system.contract.yaml` |
| the six C++ contracts (`workspaces/cpp` ×5, `workspaces/derived-tiers-cpp` ×1) | **0** |

And the check is C++-only: the header emitter is reached from
`nano_ros_node_register`, i.e. the C/C++ road (RFC-0100 D10 already records the
declared-QoS check as C++-only).

**So the check and the only declaration in the tree do not intersect.** The one
declared depth is in the language that has no check; the images that have the
check declare nothing.

## This is NOT a broken check, and an earlier draft of this issue said it was

Worth recording, because the wrong version was nearly filed. The observation
that started this was artifact state: production declared-QoS headers read
`NROS_DECLARED_QOS_STATUS "refused"` with zero rows, which was read as *"the
declared-depth check is silently OFF for every shipping C++ image"* — the
`check-no-vacuous-tests` shape, where a check passes on the very thing it exists
to catch.

Reading the CONSUMING header refutes that on every word.
`packages/api/nros-cpp/include/nros/declared_qos.hpp`:

- **Not off.** `declared_depth_or` (≈186-191) short-circuits deliberately, with
  the rationale written out: *"When nothing was declared there is nothing to
  disagree with, so the passed depth is compared with itself and the assertion
  above holds trivially — an image that has not opted in is not in error."*
  `NROS_ASSERT_DECLARED_DEPTH` opens with an explicit
  `… == DECLARED_DEPTH_UNDECLARED ||`.
- **Not silent.** `NROS_DECLARED_QOS_STATUS` is emitted into the generated header
  as a readable string — it is how the `"refused"` state was observed at all.
- **Not indistinguishable from a healthy table.** `COUNT` exists separately from
  the array's length precisely to separate the two cases, and the `#else` arm
  says so: a `COUNT` of zero is *"'the table holds nothing', a different claim
  from 'this endpoint was declared depth zero'"*.

The exact distinction the draft claimed was missing is the thing the code was
built to make. The error was inferring behaviour from ARTIFACT STATE without
reading the consumer. An opt-in check whose opt-out is a documented decision is
a POLICY; disagreeing with a policy is an RFC question, not a bug report.

## What is actually open

Two things, both smaller than the withdrawn claim.

1. **Adoption.** The mechanism's only exercise is a test fixture, so `check c` /
   `check cpp` demonstrate that it WORKS without demonstrating that any shipping
   image USES it. That is the same fact #1555 records from the other side, and
   #1555's expiry condition is this issue's acceptance: get one C++ image to
   declare a depth in its contract.

2. **Is the opted-out state discoverable?** This is the part of the withdrawn
   claim that survives, narrowed. The status IS recorded, but it is recorded in a
   GENERATED header inside a build directory. A user who writes a `qos: depth:`
   into a C++ contract and expects a build failure on a mismatch has no
   affordance short of reading that artifact to learn the check did not apply to
   them. Nobody has hit this, because no C++ contract declares a depth — so it
   is latent, not live, and it becomes live the moment item 1 is done.

   Not asserted as a defect: the cmake site is careful where it can be (a
   MISSING table is fatal, 2+ models abstains loudly). Whether an EMPTY table
   deserves a `message(STATUS)` naming the opt-out is a judgement about how loud
   an opt-in should be, and it should be decided when someone first opts in
   rather than in the abstract.

## Acceptance

One C++ image declares a depth in its contract sidecar, its generated header
reports `"resolved"` with a non-zero row count, and a deliberate mismatch in that
image fails the build. At that point #1555's blocker lifts too, and item 2 stops
being hypothetical.
