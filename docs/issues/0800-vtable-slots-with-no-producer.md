---
id: 800
title: "42 of 74 vtable slots have no producer in any backend, and the parity map
  counts them answered because a slot exists"
status: open
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
