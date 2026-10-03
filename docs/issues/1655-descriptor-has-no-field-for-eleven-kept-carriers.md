---
id: 1655
title: "Eleven KEPT `NROS_DECLARED_*` carriers have a second blocker no road fix answers — the sizing descriptor has no field, no reader, or no file for the fact they carry"
status: open
type: tech-debt
area: [build, cli, core]
severity: low
found: 2026-10-03
related: [1649, 1407, 1595, 1408, rfc-0100, phase-454, phase-457]
---

## What

`scripts/check/check-knob-single-reader.py`'s `KEPT` ledger records, per
carrier, why it cannot retire. For most rows the reason is the ROAD: a
multi-entry cmake configure names no descriptor to cargo (issue 1649, after
issue 1407's west half was fixed by PR #1601). RFC-0100 Amendment 1's D12 is
that road's remedy — every runtime build named exactly one descriptor — and for
those rows the retirement is then the W14 knob diff.

Eleven rows carry a SECOND reason in their ledger text, and D12 does not touch
it. They were filed as reason strings against 1407, so they would have resurfaced
only when 1649's retirement test was re-run, one row at a time. Grouped by what
the descriptor lacks (read from the ledger 2026-10-03):

| what is missing | carriers | the ledger's reason |
| --- | --- | --- |
| an `[image]` FIELD — the fact is image-wide and no endpoint row states it | `EXECUTOR_MAX_CBS` | sums `callback_slots()` over `Timer` and `GuardCondition`, which `endpoint_kind` drops |
| | `EXECUTOR_MAX_SC` | the scheduling-context count is the SCHEDULE (`execution.tiers`), which the schema does not model |
| | `EXECUTOR_ACTION_CLIENTS` | `heavy_slots` has no `[image]` field; counting rows and multiplying is the third mirror D4 refuses |
| | `MAX_PUBLISHERS` | no `[image]` field, so a consumer would restate the action expansion |
| | `EXECUTOR_MAX_MONITORS`, `EXECUTOR_MAX_AGE_MONITORS` | counts of CONTRACT rows (`monitor_rows` / `age_rows`), not of endpoints |
| | `INFRA_QUERYABLES` | a FEATURE token from `execution.features`, not a count |
| a per-row COMPONENT attribution | `RUNTIME_MAX_CELL_ENTITIES` | a max over per-component per-kind counts; `[[endpoint]]` rows carry no component |
| a READER — the descriptor states it, nothing ranks it first | `SERVICE_SERVERS` | `queryable_floor_from` takes the count from the carrier alone |
| | `MAX_QOS_DEPTH` | the reduction it carries (the MAX, guarded on every subscription declaring) is a consumer-side restatement nothing shares |
| a FILE — the producer writes none where the carrier still delivers | `NODES` | emitted for a model that describes no wiring, which is exactly where "no contract ⇒ no file" (RFC-0100 D4) writes nothing |

## Why it is one issue and not eleven

Each fix is small, but they share two rulings that should be made once:

1. **`[image]` grows, and by what rule.** D4 admitted `[image]` for "the counts
   no endpoint row can" carry (node, backend, subscriber). Seven rows above are
   the same kind of fact. Each field must be COMPOSED where the count is already
   derived (`EntityInventory::derive`), never re-derived from rows by a
   consumer — the ledger's own "third mirror" rule — and each needs a D12
   reduction for an N:1 runtime (the per-component `merged_per_kind_max` already
   covers counts).
2. **"No contract ⇒ no file" vs a fact that needs no contract.** The node
   count is known from `nros-metadata.json` alone. Either the rule becomes "no
   STATED fact ⇒ no file" (and the `[meta] basis` guard every consumer reads has
   to say which facts a contract-less file states), or `NODES` stays a carrier
   by design and moves to a `ByDesign` row with that reason. Decide; do not let
   it ride as a 1649 row.

## Acceptance

Every row above either retires (the W14 method: the descriptor named, the
carrier removed, each consumer's emitted knobs diffed equal on a named image) or
moves to a `ByDesign` ledger row stating why the descriptor will never carry it.
The ledger then holds no row whose second reason is a missing field, reader or
file.

## Order

After issue 1649 (D12): until a multi-entry configure names a descriptor, these
rows cannot retire on that road whatever fields exist, and adding fields first
risks a reduction that disagrees with D12's.

## Rows added 2026-10-03, from the D12 knob diff (issue 1649)

Re-running the per-carrier test once the multi-entry road named a descriptor
moved three more rows here, each for a reason of this issue's kind:

| what is missing | carrier | measured on `examples/workspaces/cpp` native |
| --- | --- | --- |
| a READER — `[image] node_count` states it | `EXECUTOR_MAX_NODES` | dropping it: `nros-node` `MAX_NODES` and zenoh `MAX_PER_NODE_LIVELINESS` 6 → 4 |
| a READER — `[image] subscriber_count` states it | `MAX_SUBSCRIBERS` | dropping it: `ZPICO_MAX_SUBSCRIBERS` 2 → 8 |
| a FILE — emitted whenever the bound closure derived, contract or not | `SUBSCRIPTION_BUFFER_SIZE` | field + reader landed (issue 1595); zero-diff where a descriptor is named |

## Progress, 2026-10-03 — both rulings made; seven fields, eight readers; four rows left

**Rulings** (recorded in RFC-0100, "Ruling, 2026-10-03"):

1. `[image]` grows by one rule — a field is a `DerivedEntityKnobs` fact a
   carrier already delivers, stated iff `derive` derived, refused with its
   reason otherwise, composed on an N:1 runtime by D12's fold. Corollary: a
   field takes over a delivered fact and never introduces one, so the
   standalone-leaf DECLARATION road refuses the new fields (no carrier ever
   sized those images; `--from-leaf` over the twelve NuttX C/C++ leaves would
   have stated `publisher_count` 1 / `callback_slots` 1 for a talker against
   builtins of 8 / 4).
2. "No contract ⇒ no file" restated as "no STATED entity fact ⇒ no file" —
   what both producers already do. The `examples/native/rust/talker`
   observation (`basis = "contract"`, no QoS, after its contract was removed)
   is the cargo-leaf probe road writing what its probe states — re-measured on
   a fresh `nros sync` with no contract — not a stale file and not a writer
   bug; `Basis::Contract`'s doc now says "this image's own endpoint set". A
   closure-only or model-only fact does not earn a file, so `NODES` and
   `SUBSCRIPTION_BUFFER_SIZE` are `ByDesign(RFC-0100 D4)` rows.

**Fields** (`[image]`, additive): `callback_slots`, `action_client_slots`,
`publisher_count`, `sched_context_count`, `monitor_rows`, `age_monitor_rows`,
`cell_entities`, each from `derive`; projected to cmake as
`NROS_SIZING_IMAGE_*`.

**Readers, descriptor first and the carrier next:** `nros-node` (`MAX_CBS`,
`MAX_SC`, `MAX_MONITORS`, `MAX_AGE_MONITORS`, `MAX_NODES`,
`ARENA_ACTION_CLIENTS`), `nros-rmw-zenoh` (`MAX_PER_NODE_LIVELINESS` from
`node_count`), `nros-zpico-build` (`ZPICO_MAX_PUBLISHERS` /
`_SUBSCRIBERS`, floored at 1 as before).

**Measured**, the `examples/workspaces/cpp` native runtime's own `nros_cpp`
cargo command, runtime descriptor named, each carrier dropped alone and all
eight together: **0 generated files differ** for `EXECUTOR_MAX_CBS`,
`EXECUTOR_MAX_SC`, both `MONITORS`, `EXECUTOR_MAX_NODES`,
`EXECUTOR_ACTION_CLIENTS`, `MAX_PUBLISHERS`, `MAX_SUBSCRIBERS` (before the
readers: 9→4, 7→8, 0→8, 0→8, 6→4, 2→9, 3→8, 2→8). Those eight rows moved to
issue 1649's retirement step.

**Still open here — four rows:**

| carrier | what is missing |
| --- | --- |
| `RUNTIME_MAX_CELL_ENTITIES` | a READER: `[image] cell_entities` is stated; `packages/api/nros/build.rs` has no `nros-sizing-descriptor` build-dependency, and adding one re-resolves every leaf lockfile that pulls `nros` (`just lock-update`) |
| `SERVICE_SERVERS` | the app service-server count is `service_server_entities` + the action servers' three channels each; stating it needs a field with that expansion owned by the producer, not a reader multiplying rows |
| `INFRA_QUERYABLES` | a FEATURE token (`execution.features`), not a count — needs a vocabulary field, not an `[image]` count |
| `MAX_QOS_DEPTH` | the guarded MAX over subscription depths has no shared spelling; a field would be the reduction moved into `derive` |
