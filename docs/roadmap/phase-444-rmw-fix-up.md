# Phase 444 — RMW fix-up: what the contract report shows, and what the issue records hid

**Status (2026-09-10). Not started. Opened from a review of the RMW report; supersedes
phase-393, which is archived in the same change.**

Implements RFC-0054 (the C headers are the ABI SSoT). Continues phase-393 (archived).

## Why this phase exists

Phase-393 closed on *"the contract work this doc scoped is finished — what remains is
VERIFICATION"*, and the tools still agree. That claim holds. What it did not cover is
everything between "the slot exists" and "the slot is right": per-backend coverage the
aggregate hides, correctness issues with no home or the wrong home, and issue records
that lag the code.

## The report, measured 2026-09-10

| tool | reading |
| --- | --- |
| `just check rmw-api-parity` | 88 contract symbols, **0 gap**, 20 declined with reasons, 7 answered by an inert slot |
| `just check rmw-abi-shape` | 65 mirrored: 15 identical, 39 with declared arg differences, 9 grouped, **0 undeclared differences, 0 missing slots** |
| `just check rmw-slot-producers` | 68 slots: 58 produced, 4 default, 6 inert |

Phase-393's own status block had drifted from this (it read produced 53, default 7,
inert 14, and listed as inert the on-new-* callbacks, content filter and network-flow
slots that phase-407 declined and removed). Per backend, of 68 slots:

| backend | filled | graph |
| --- | --- | --- |
| cyclonedds | 39 | **1 / 12** |
| rust-adapter (every Rust backend) | 42 | 11 / 12 — a trampoline per slot, so this row cannot answer per backend |
| xrce | 24 | 0 / 12 — declared `UNSUPPORTED`, tested |
| uorb | 18 | 0 / 12 |

## The records lagged the code

Of the 23 open issues in the `rmw` area, **9 had a `fix(...)` commit naming them**. A fix
commit is evidence, not a verdict: #1127 has one and its lane still stops before the
cells. Each was read against its own acceptance:

| issue | verdict | basis |
| --- | --- | --- |
| #1008 | **closed here** | `3941b569a2` deleted `is_server_ready`; `wait_for_service` takes `Ok(true)` only (`handles.rs:2192`); `cffi/tests/server_available.rs` updated in the same commit |
| #1237 | **closed here** | its own Status section: wake slot declined on the board, before/after measured |
| #0969 | **closed here** | both receive paths take wire CDR from the serdata (`subscriber.cpp`, and `take_typed_wire` via `41195b84f8`); cost measured in the issue; publish half was #0970 |
| #1088 | **open — half fixed** | `bd9948e23d` reserves the slot before the destructive take, so nothing is lost; the adapter still maps `WOULD_BLOCK` to `taken = false` + `OK`, which the issue's own Fix rules out; no regression test |
| #0902 | open | two leak arms fixed (`b56e3d50a8` and its predecessor); the 20–90 % symptom never re-measured |
| #1039 | open | 1 of 4 revised-acceptance boxes ticked |
| #1139 | open | "Acceptance, still unmet" |
| #1127 | open | the live-peer lane still stops in its fixture build |
| #0852 | open, **not this phase** | Zephyr transport priorities; 1 of its 5 fix items landed |

## Work items

### W1 — #1088's other half

The Cyclone `service_take_request` adapter still collapses `WOULD_BLOCK` to
`taken = false` with `OK`. That is now contract-correct about *consumption* — the sample
stays on the reader for the next take — but a saturated server is indistinguishable from
an idle one, which is the half the issue's Fix names. Surface exhaustion (a distinct
code or a counted, logged condition) and add the regression test the fix never got.

**Acceptance.** A test with more than `kRequestSlots` requests outstanding that asserts
none is lost AND that exhaustion is observable; it fails against the current adapter.

### W2 — #0902, measured

The mechanism fixes landed without a re-measurement of the symptom. This needs a router
and a live peer, which is why it has not happened.

**Acceptance.** The goal completion rate, measured on a freshly built image over enough
runs to separate it from the 20–90 % the issue recorded.

### W3 — Cyclone's graph reader (11 of 12 slots)

Cyclone fills `get_node_names` and leaves eleven graph slots `nullptr`
(`nros-rmw-cyclonedds/src/vtable.cpp`). Phase-381 W5 scoped one slot, and #0791 and #1137
are both resolved — #1137 correctly ruled the eleven `UNSUPPORTED` answers intended — so
**nothing owns the rest**. A Cyclone node appears in `ros2 node list` and cannot say
whether anything subscribes to its topics: the asymmetry phase-381 named as the defect.
Needs a reader for `ros_discovery_info`, which `graph.cpp` only writes.

**Acceptance.** `rmw-slot-producers` shows cyclonedds graph 12 / 12, and
`native-graph-rust-cyclone-r2n` asserts beyond node names against a live peer. This is
phase-sized and may be split out.

### W4 — gates whose reach is narrower than their rule

* **#1092** — `rmw-abi-shape` licenses a deviation without pinning it; the "39 declared"
  figure above rests on those declarations.
* **#1219** — RFC-0071's `check-rmw-agnostic` was never written.

### W5 — #1021, carried from phase-393

zenoh-pico 1.8.0 does not build with `Z_FEATURE_MATCHING=0`, which Zephyr passes.
Phase-393 adopted it as a backend-build contract; archiving 393 moves it here.

## A lesson the review surfaced

`#[must_use]` went on `Executor::declare_parameter` (phase-428 W6) with a check of
`cargo check -p nros-node --features std`. The two callers that dropped the value were in
another crate, behind `param-services`, inside a module gated on `rmw-cffi` — never
compiled by that check, which therefore passed. Every `nros` build with those features
then failed on main, including the live-peer lane (fixed in PR #855). A check that does
not enable the features that compile the callers checks nothing: the same "clean over code
it never built" shape as the lane that counted skips as passes.

## What this phase does NOT do

* Change the contract. 0 gap stays 0; the 20 declined stay declined with their reasons.
* Verify produced slots against live peers in general — phases 433 and 441.
* Platform build issues (#1039 NuttX, #0852 Zephyr priorities).
