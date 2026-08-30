---
id: 800
title: "34 of 74 vtable slots are written by nothing and read by nothing, and
  the parity map counted them answered because a slot exists"
status: resolved
type: tech-debt
area: rmw
related: [phase-376, issue-0781, issue-0777]
---

## Problem

Issue 0781 found that "the slot exists" was being read as "the capability
works" for the subscription loan pair. Counting the rest of the vtable the same
way, measured 2026-08-26 against every backend initializer
(`packages/rmw/*/*/src/vtable.{c,cpp}` plus `RustBackendAdapter::<R>::VTABLE`):

**42 of 74 slots are NULL in every one of them.**

Filled per backend: rust_adapter 30, cyclonedds 25, xrce 23, uorb 17.

The 42 are not one thing, and that is what makes the number useless as it
stands. Three kinds are mixed together:

1. **Optional by design, with a documented runtime default.**
   `get_implementation_identifier` says so in its own doc — NULL means the
   runtime answers with the name the backend registered under. Nothing is
   missing here.
2. **Answered at another layer.** The graph queries (`get_node_names`,
   `count_publishers`, `get_topic_names_and_types`, …) are the parity map's
   `layer` bucket — cyclonedds answers them in `graph.cpp`, not through the
   vtable.
3. **Actual gaps.** `set_log_severity` is the clearest: it landed during this
   same campaign (phase-376 W5) with a header slot, a runtime dispatcher at
   `packages/rmw/cffi/src/lib.rs:3316`, and a test built on stub vtables — and no
   backend body, though Cyclone has `dds_set_log_mask`, zenoh-pico has a log
   level, and the XRCE client has one. `rmw-api-parity.py` maps it
   `("vtable", "set_log_severity")`, so it counts as answered.

A reader cannot tell which kind a given slot is without opening it. That is the
same defect 0781 named, one level up: the bucket `vtable` means "a slot exists
in `nros/rmw_vtable.h`", which is true of all 42, and says nothing about whether
anything fills it.

## Why the obvious gate is not enough

A check that fails when a slot has no producer would fire on all 42 immediately
and be baselined into silence within a day. The useful shape is a DECLARED
table — slot → (optional-with-default | answered-at-layer-X | gap, with a
reason) — and a gate that fails when the computed set and the declared set
disagree in either direction. That is 42 authored reasons, and authoring them
carelessly reproduces exactly the failure 0777 recorded: a reason nobody checked
staying wrong for years while the conclusion it supports stays green.

## Direction

1. Author the table, one reason per slot, in the same file as the computation so
   the two cannot drift.
2. Gate it both ways: a new slot nobody fills must be classified, and a slot
   that GAINS a producer must lose its entry.
3. Wire the kind-3 gaps that have an obvious backend primitive. Start with
   `set_log_severity` on cyclonedds (`dds_set_log_mask`) — it is the one this
   campaign shipped incomplete.
4. Make the parity map distinguish "a slot exists" from "a backend fills it".
   Today `vtable` conflates them, which is what let 0781's loan trio and
   `set_log_severity` read as coverage.

## Measurement, for re-running

The count came from parsing the header's slot order (reuse
`check-vtable-positional-order.py`'s `header_field_order()`) and, per backend
file, the slots assigned something other than `nullptr` / `NULL` / `None` in
either the positional `/*name*/ value,` style (cyclonedds, uorb) or the
designated `.name = value,` / `name: value,` style (xrce, rust_adapter).
Multi-line initializer entries are not matched by that scan; none exist today,
but a real gate must handle them rather than silently undercounting — an
undercount here reads as a gap that is not there.

## Corrected measurement, 2026-08-26 — the number was 42, and it was the wrong number

"42 with no producer" mixed three states, which is why the issue said a reader
cannot tell which kind a slot is. Splitting by the second question — does
anything READ the slot — separates them, and both halves are derived from the
tree rather than declared:

| | count | meaning |
| --- | --- | --- |
| produced | 32 | a backend's vtable assigns it |
| default | 8 | consumed, no producer, header documents what NULL does |
| unimplemented | 0 | consumed, no producer, NULL behaviour undocumented |
| **inert** | **34** | **no producer AND no consumer** |

So the headline is not "42 have no producer". It is that **34 of 74 slots are
written by nothing and read by nothing** — declared in the header, generated
into the Rust bindings, and touched by no code in the tree. Reserving a slot's
position and shape before anything fills it is legitimate for an ABI that
mirrors upstream; what was missing is that nothing distinguished reserved from
working.

`scripts/check-rmw-slot-producers.py` (`just check rmw-slot-producers`, fast
line) is that distinction, and it is two-way: an inert slot in no declared
family fails, and a family naming a slot that stopped being inert fails.

### What the split found that the count could not

**`destroy_node` was a leak, not a reservation.** `create_node` is consumed
(`lib.rs:1510`) and `destroy_node` was consumed nowhere, so `close()` cleared
the shim's node slot and never told the backend. Any backend allocating state in
`create_node` lost it on every session close. It read as an optional slot
precisely because it had no producer AND no consumer — the state this issue is
about. Fixed: `release_session_nodes` now dispatches `destroy_node` for each
node the session created, before `destroy_session` (a backend's node state hangs
off its session state), and the header documents what a NULL slot means there.
Test `closing_a_session_destroys_the_nodes_it_created` fails against the pre-fix
code, 0 destroys against 2.

**`graph.cpp` existing is not the graph queries being implemented.** The parity
map and CLAUDE.md both suggest the graph functions are answered one layer down
in `nros-rmw-cyclonedds/src/graph.cpp`. That file exports `graph_init`,
`graph_track_{writer,reader}`, `graph_publish` — it PUBLISHES this node's
participant info so other nodes can see it. It implements none of
`get_node_names`, `count_publishers`, or the `*_names_and_types` family. Those
nine plus the two counts are inert, and reading the graph back means holding a
discovered view of every peer, which is why they are reserved rather than
planned.

### Item 4, done differently than proposed

The issue asked the parity map to distinguish "a slot exists" from "a backend
fills it". Not by changing the bucket: `check_against_vtable` rejects a `gap`
whose slot exists, and it is right to — the bucket answers "where do we answer
this", and a declared slot IS where. So the second dimension is printed beside
the first instead. `rmw-api-parity.py` now reports:

```
  vtable     70
  ...
  NOTE  `vtable` counts a SLOT, not a backend that fills one. Of 74 slots:
        32 filled by some backend, 8 NULL with documented behaviour,
        34 written and read by nothing.
```

70 can no longer be read as 70 working.

### Still open

Item 3, the kind-3 gaps with an obvious backend primitive. `destroy_node` was
the one that was actually broken and is fixed. `set_log_severity` remains: slot,
dispatcher and stub tests since phase-376 W5, no backend body, though Cyclone
has `dds_set_log_mask`, zenoh-pico has a log level and the XRCE client has one.
Its NULL behaviour IS documented (the runtime answers `UNSUPPORTED`), so the ABI
is honest about it — it is a missing feature, not a lie, which is why it did not
block closing the rest.

## `set_log_severity` implemented on cyclonedds — 2026-08-27

The one item this issue left open. It had a slot, a runtime dispatcher
(`cffi/src/lib.rs`) and stub-vtable tests since phase-376 W5, and no backend
body — so every real image answered `UNSUPPORTED` while Cyclone has had
`dds_set_log_mask` the whole time.

Cyclone's control is a CATEGORY BITMASK, not a level ladder, so the ladder maps
onto cumulative masks — each severity enables itself and everything more urgent:

| severity | mask |
| --- | --- |
| FATAL | `DDS_LC_FATAL` |
| ERROR | `+ DDS_LC_ERROR` |
| WARN | `+ DDS_LC_WARNING` |
| INFO | `+ DDS_LC_INFO` |
| DEBUG | `DDS_LC_ALL` |
| UNSET | refused, `INVALID_ARGUMENT` |

DEBUG opens everything because every category outside FATAL/ERROR/WARNING/INFO
falls into trace (`ddsrt/log.h`). UNSET is refused rather than guessed: it means
"no severity stated", and a backend inventing one picks a verbosity nobody asked
for.

Filling it meant naming the 37 slots after `process_raw_in_place`: the table is
POSITIONAL and `set_log_severity` is slot 73 of 74, so reaching it cannot skip.
`check-rmw-vtable-order` now checks all 74 against the header, which is
strictly better than a table that stopped early and leaned on
value-initialisation — an upstream insertion can no longer shift the tail
silently.

Measured effect on this issue's own numbers: **produced 32 -> 33, default
8 -> 7**. `set_log_severity` is no longer a documented-NULL.

Test `nros_rmw_cyclonedds_log_severity` reads the mask BACK with
`dds_get_log_mask()` rather than trusting the return code — the return says the
call was made, the mask says what it did, which is the distinction issue 0803
spent a day inside. It also asserts a refused UNSET leaves the mask untouched,
so the refusal cannot be a no-op that changed state anyway. Verified
load-bearing: a deliberately wrong WARN mapping fails it.

### What stays open, and is now the whole of it

The 34 inert slots. They are declared with a reason each in
`check-rmw-slot-producers.py`'s families and the gate keeps that honest in both
directions, which is what this issue asked for. Wiring any of them is a feature,
not a defect — the ABI no longer claims they work.
