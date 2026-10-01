---
id: 1564
title: "The declared-depth `static_assert` is opt-in and NOTHING has opted in — the only `qos: depth:` in the tree is in the one language the check cannot reach"
status: resolved
resolved_in: 2026-10-01
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

## Resolution (2026-10-01)

**The reason nothing had adopted it was not only policy, and measuring the
native road found it.** `_nros_emit_declared_qos_header` ABSTAINED whenever a
configure had resolved more than one SystemModel, and the native road builds
every entry of a workspace in one configure: `examples/workspaces/cpp` resolves
seven, and its configure printed "not rendering the declared QoS depths of
`<component>` ... Every declared-depth check in it is OFF" for all six of its
components (measured before this change). So a C++ image on the native road
COULD not adopt the check, whatever its contract said.

**The class fix.** `nros ws entity-inventory --model` is repeatable for the two
per-component outputs, and the cmake renderer hands it every distinct model.
The table is the UNION per `(type, topic)` and per column
(`DeclaredQosHeaderTable::union`): a model that states nothing for an endpoint
takes another's declaration (silence is not a declaration, and the table only
ever holds the component's own code to it -- the same code in every image), an
equal statement is no conflict, and two models stating DIFFERENT values refuse
the whole table, naming both, with a `nros: declared QoS: no table for ...`
line on the configure output. A model that describes no wiring at all declares
nothing and is skipped. The declared-PARAMETERS header (same render, same
abstention) got the same union by node FQN. Every image-wide output of the verb
refuses a second `--model`: a merge of several images' counts was never
defined (and the one place this configure DOES merge them is issue 1607).

**The adopter.** `examples/workspaces/cpp`: `system.contract.yaml` declares
`listener`'s `chatter: { qos: { depth: 1 } }`, and `Listener.cpp` states
`constexpr nros::QoS kChatterQos = nros::QoS(1)`, asserts it with
`NROS_ASSERT_DECLARED_QOS` (issue 1256's all-columns macro) and registers with
it. Measured on the native fixture build (`workspace-fixtures-build.sh linux
cpp --id workspace-cpp-native`), against the acceptance line by line:

| acceptance | measured |
| --- | --- |
| one C++ image declares a depth in its contract sidecar | `examples/workspaces/cpp/src/demo_bringup/launch/system.contract.yaml` |
| its generated header reports `"resolved"` with a non-zero row count | `pkg/listener_pkg/nros-declared-qos/listener/nros/nros_declared_qos_generated.h`: `NROS_DECLARED_QOS_STATUS "resolved"`, `NROS_DECLARED_QOS_ROW_COUNT 2`, rendered from the union of the seven models |
| a deliberate mismatch in that image fails the build | contract `depth: 2`: the build fails in `Listener.cpp` with `declared_depth_agrees<2, 1>` and `"/chatter"`; contract `reliability: best_effort` beside `depth: 1`: fails with `declared_reliability_agrees<1, 0>`. Restoring the contract builds clean |

Runtime: `native_robot1_entry` + `native_robot2_entry` (the listener at
KEEP_LAST(1)) delivered 7 of 7 samples through `rmw_zenohd`. `native_entry`
itself dies at boot with `NodeError::ExecutorFull` -- PRE-EXISTING, measured
identically on the main checkout's 2026-09-29 build of the same directory, and
filed as issue 1607 (the configure's image-wide entity fragment is
last-entry-wins).

**Gates.** `just check declared-qos-header` case H drives the several-model
seam end to end -- the union carries the one declaration among three models,
and a conflicting fourth refuses the table and says so without failing the
configure; with the old abstention planted back it goes red on 4 of its 49 assertions.
`packages/cli/nros-cli-core/tests/declared_qos_adopter.rs` resolves the
adopter's REAL launch files with the pinned resolver, renders the listener's
union table and asserts the row -- and that `Listener.cpp` still asserts
against it, so the adoption cannot quietly decay back to a number nothing
checks; it went red with the contract row reverted.

**Item 2, decided now that someone has opted in.** An EMPTY table stays silent:
it is what every component of every image without a contract gets, and a line
per component per configure would be noise that teaches people to ignore the
one that matters. The states where a declaration exists and the check is OFF
are what get said out loud: a model conflict prints `nros: declared QoS: no
table for <component>` (and the header says why), as the abstention it replaces
did. A call site that states its QoS without the macro is not a silent opt-out
either: it reaches the FFI registration check, which refuses a disagreement at
boot.

#1555's blocker is about the metadata `entities` key and the committed compile
fixture, and is unchanged by this: the fixture header was regenerated for the
new row format and is still rendered from `entities.json`.
