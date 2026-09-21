---
id: 1419
title: "No layer reconciles the contract's declared endpoints with what the node
  code creates; the first catch of an omitted subscription is ExecutorFull at
  boot"
status: open
type: bug
area: [build, cmake, testing]
severity: high
found: 2026-09-21
related: [0257, 0965, 1084, 0641, 0304, phase-308, phase-313, phase-403, phase-446, phase-463, rfc-0100]
---

## Problem

Since phase-412 the contract sidecar (`<bringup>/launch/<stem>.contract.yaml`)
is the sole source for every pool an image compiles with: `nros ws
entity-inventory` counts subscriptions, publishers, queryables, callback slots
and liveliness tokens from it, and the inventory header says on purpose that
it carries NO headroom, "so a stale declaration makes the image fail entity
creation with `ExecutorFull` naming this knob, rather than being absorbed
silently". The design assumes the declaration equals what the code creates.
Nothing on the host checks that it does, in either direction.

## Evidence

Measured on the safety-island consumer (four C++ nodes, one contract, native
and Zephyr images), 2026-09-20, three edits to the contract and none to the
code:

| edit | resolver | inventory | entry codegen | first catch |
| --- | --- | --- | --- | --- |
| delete `mrm_handler/operation_mode_state` and its topic (the 2026-09-04 seventh-subscription bug, replayed) | 0 errors | `MAX_SUBSCRIBERS 11 -> 10`, `EXECUTOR_MAX_CBS 19 -> 18`, `MAX_LIVELINESS 58 -> 57`, arena `50640 -> 46976` | rc=0 | `ExecutorFull` at boot, on a board with no wired console |
| declare a `phantom` sub the code never creates, wired to a topic | 0 errors | every pool one longer, arena +3664 B | rc=0 | never |
| declare the same sub under `sub:` with no `topics:` row | 0 errors | unchanged (counts come from the wiring) | rc=0 | never; the endpoint is dropped with no diagnostic |

The one host-time cross-check that exists, `NROS_ASSERT_DECLARED_DEPTH` in
`NROS_SUBSCRIBE` (phase-403 step 2), is keyed on the code's call sites and by
its own rule ("an ABSENT row is nobody declared this endpoint, and nothing
asserts against it") cannot see an endpoint the code never mentions or a
call site the contract never mentions. The declared-params check (phase-446
W6, `DECLARED_PARAM_MISMATCH -446`) has the same one-sided shape.

## Why the existing recorder does not cover it

phase-308 built the entity recorder (`metadata-mode`: a recording RMW
backend plus three executor-side hooks in `packages/api/nros-cpp/src/metadata_hooks.rs`)
and phase-313 the C/C++ probe that runs it from `nros sync`. On the island,
today:

* the probe FAILS for all four components (`error[E0428]:
  builtin_interfaces_msg_duration_t defined multiple times` - the batch
  project's per-package FFI glue crates collide on a shared interface type),
  issue 0641's negative cache stops retrying, and sync continues with "no
  producer", which is its documented fallback;
* the two sidecars that survive from 2026-09-11 record `depth: 10` for a
  subscription the code creates at `::nros::QoS(1)`, because
  `nros-rmw-metadata::create_subscriber` takes `_qos` and never reads it
  (`packages/rmw/metadata/src/lib.rs:131`);
* `parameters: []` in both, because no hook observes
  `nros_cpp_node_declare_param_*`;
* the only consumer of a recorded count is
  `count_callbacks_with_recorded` (`packages/core/nros-orchestration-ir/src/executor_sizing.rs:154`),
  which takes `max(modelled, recorded)` for one number and therefore cannot
  report a disagreement; `packages/cli/nros-cli-core/src/entity_inventory.rs`
  never reads a recorded sidecar at all.

So the seam that could reconcile exists, is incomplete, is broken on the
reference consumer, and feeds nothing that compares.

## Current state

Open. The island's native `just build` exports `NROS_EXECUTOR_MAX_CBS=32`,
so its native image is hand over-provisioned and would not have shown the
omission even at boot; the Zephyr image (derived 19) would have, on silicon
with no console.

## Fix / direction

[phase-463](../roadmap/phase-463-host-census-reconciles-contract-with-code.md):
complete the recorder (QoS as passed, a parameter hook, timer kinds), make the
generated native ENTRY the census producer (same binary as boot, switched by
`NROS_CENSUS_OUT` in the hosted funnel, where the `env` capability already
lives and no RTOS board has one), and add `nros ws entity-census check`
emitting one verdict per (node, kind, name) - `missing-in-contract`,
`phantom`, `unwired`, `depth-mismatch`, `type-mismatch`, `period-mismatch`,
`param-*` - with the UNDER direction an unwaivable error and the OVER
direction waivable per row. The RTOS configure requires a fresh census
(content-addressed, RFC-0063-shaped provenance). Nothing is added to the RTOS
image: the feature is on the native umbrella only, and phase-463 W5 gates that
with a feature-set check, an `nm` check and the image-facts byte comparison.

Acceptance for closing this issue is phase-463 W3's: the three edits above
each refuse on the host with one named row, before any RTOS image is
configured.
