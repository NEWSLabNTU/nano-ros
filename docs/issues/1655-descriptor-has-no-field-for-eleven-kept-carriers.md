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
