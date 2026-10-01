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
related: [0257, 0965, 1084, 0641, 0304, phase-308, phase-313, phase-403, phase-446, phase-463, rfc-0100, 1600]
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

### 2026-10-01 -- the census reaches a real cross configure

phase-463 W1-W4 landed (the recorder, the native entry as producer, the
verdict check, the configure-time `--require-fresh` call -- the last as PR
#1233), each gated on a FIXTURE. Run end to end on an in-tree workspace for
the first time, the road had three defects, each of which on its own meant no
cross configure ever compared anything:

1. **The C++ entry the workspaces generate produced no census.** The typed
   single-executor C++ native entry calls the header-only
   `nros::board::LinuxBoard::run_components`, which never reached the Rust
   funnel W2 put the `$NROS_CENSUS_OUT` switch in. Measured on
   `examples/workspaces/cpp` `native_entry`: the run ignored the variable,
   dialled zenoh and exited 156 on `ConnectionFailed`, writing nothing.
2. **The producer and the consumer named different files.** Both sides keyed
   the census by `<their own build dir>/nros/census/<their own entry>.json`,
   so the native run wrote `.../native_entry.json` and the threadx configure
   looked for `build/threadx-linux-zenoh/cmake/nros/census/threadx_entry.json`
   -- and its remedy said `nros ws entity-census run --entry threadx_entry`,
   an entry with no host binary. Every cross configure read "census missing",
   which the landing `[census] on_missing = "warn"` lets build.
3. **`run` could not find a binary `nros build` had built**
   (`build/<image-root>/cmake/<entry>`), so the documented remedy refused.

Fixed in the PR that carries this section (*the census reaches a real cross
configure*): `LinuxBoard::run_components` calls `nros_cpp_census_begin` /
`nros_cpp_census_finish` (the funnel's own three functions, no second census
path); the census of a MODEL lives beside it
(`<model-dir>/<stem>.census.json`, `census_path_for_model`), which is the one
document both images of one launch file share, and cmake ASKS for that path
(`nros ws entity-census path --model`) instead of spelling one; `run` searches
`nros build`'s image roots and refuses an ambiguity by name.

Measured on `examples/workspaces/cpp` (native image + `threadx-linux` cross
configure, census taken by `native_entry` against the pristine contract):

| edit to `system.contract.yaml` | threadx configure, before | after |
| --- | --- | --- |
| none | `census missing (WARNING)`, builds | `3 confirmed, 0 error(s)`, builds |
| E3a: drop `listener.sub.chatter` + its wiring | `census missing (WARNING)`, builds | REFUSED: `error missing-in-contract listener sub /chatter`, not waivable, remedy names the two lines |
| E3b: declare + wire `listener.sub.phantom` | `census missing (WARNING)`, builds | REFUSED: `error phantom listener sub /phantom`, waiver key printed |
| E3c: declare `listener.sub.phantom`, no `topics:` row | `census missing (WARNING)`, builds | REFUSED: `error unwired listener sub phantom` |

**The census producer is sized by the contract it checks.** That is the
fourth defect, and it is the one that makes E3a reach the census run itself
when the native image is REBUILT from the edited contract: the native image's
callback table derives one short and the census run stops at `ExecutorFull`
(measured: exit 250, `2 node(s), 0 sub / 1 pub / ... / 1 timer`). Before, it
discarded what it had and the configure read "census missing" (warn). Now the
run writes what the recorder saw, marked `incomplete`, exits non-zero, and
every check against it refuses (`census INCOMPLETE`); `nros sync` reports it
as "current but INCOMPLETE". Measured: the threadx configure REFUSES. It does
not name the omitted row -- the executor refuses a registration
(`next_entry_slot`, 14 sites in `nros-node`) before the recorder sees it.

Gates: `check-entity-census` now never names the census path on either side,
takes it with one entry name and checks it with another (the real road), and
gains move 4b (an incomplete census is written and refused);
`workspace_metadata::cmake_cpp_workspace_entry_writes_a_census_without_a_router`
runs the PREBUILT C++ workspace fixture in census mode (negative control
measured: with the header change reverted it fails, exit 156, no file).

### What is left (why this stays open)

* **`on_missing` / `on_stale` still land as `warn`.** A workspace that never
  takes a census still builds its RTOS image unchecked; the original failure
  (`ExecutorFull` on the board) is reachable by not running the producer.
  phase-463 W6 (the island flip, then the default) is not started.
* **E3a after a native rebuild refuses as INCOMPLETE, not as one named
  `missing-in-contract` row.** Either the recorder records a REFUSED
  registration (the 14 `next_entry_slot` sites), or the census-capable native
  image is sized independently of the contract it checks. Neither is done.
* **Rust and C entries have no census.** `boot_hosted` refuses
  `$NROS_CENSUS_OUT` for a Rust entry; a C entry's entities reach the
  recorder with no node attribution (phase-463 Limits). The class is checked
  for C++ entries only.
* **phase-463 W5** (the RTOS-image-unaffected invariants as gates) is not
  started; the new FFI pair is `#[cfg(feature = "env")]`, which no RTOS
  umbrella has, but nothing gates it.
* **Issue 1600** (found here): a multi-entry configure sizes its one runtime
  from the LAST entry's model, so `examples/workspaces/cpp`'s `native_entry`
  dies at boot with `ExecutorFull` on the pristine contract -- and so does its
  census run. The measurements above used `NROS_EXECUTOR_MAX_CBS=8` stated for
  the native build to step around it.

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
