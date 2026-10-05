---
id: 1701
title: "A multi-entry configure sizes its shared runtime for the UNION of its entries, not the largest one — `MAX_CBS` 9 where one entry needs 2"
status: resolved
resolved_in: 2026-10-06
type: tech-debt
severity: low
area: [build, cmake, sizing]
related: [1600, 1649, 1564, rfc-0100, phase-457]
found: 2026-10-05
---

## What

A configure that builds several entries links ONE runtime into all of them
(RFC-0065 D8), and since issue 1600 (PR #1557) that runtime is sized from
`EntityInventory::shared_runtime_over`: every entry's model folded in through
`merged_per_kind_max`, so a component any entry launches is launched. RFC-0100
Amendment 1 D12 (issue 1649, PR #1615) composes the shared runtime's sizing
descriptor over the same reduction.

The reduction is a UNION of the images. Each process runs exactly ONE entry,
so the true need is the LARGEST single entry, and the union SUMS where that
need is a max. Issue 1600 chose the union deliberately — it is the only
reduction that cannot under-size, and under-sizing was the bug — and recorded
this cost as not done.

## Measured

`examples/workspaces/cpp`, native configure (seven entries, one runtime):

| | value |
| --- | --- |
| `NROS_DERIVED_EXECUTOR_MAX_CBS` (union, issue 1600) | 9 |
| `native_entry`'s own need (`resolved.toml` `max_cbs`, talker timer + listener sub) | 2 |
| executor arena, carriers only (issue 1600 measurement) | 66,816 B, of which 752 B claimed at first spin |
| executor arena, D12 runtime descriptor named (issue 1649 measurement) | 36,488 B |

The per-entry maxima of the other six entries were NOT measured; "9 against
2" compares the union with one entry, not with the largest.

## Why it is low severity

It is the safe direction: an over-sized pool wastes RAM, it never fails a
registration. And it only arises on a multi-entry cmake configure, which today
is the NATIVE coordinate of a workspace — a host build, where RAM is not the
constraint. A cross configure with several entries would pay it on a target.

## Fix direction (not decided)

A tight answer is a per-KNOB max over each entry's OWN derivation: derive each
entry alone, then take the max of each count. That is not a drop-in change,
because the fragment and the descriptor are not only counts — declared-QoS
tables, subscribed/received type lists, per-component rows and the
`NotLaunched` classification are per-endpoint facts, and D12 already REFUSES a
per-endpoint fact the entries disagree on. So it needs a second, field-by-field
reducer beside `derive`, with a rule per field (max for counts, union for type
lists, refuse-on-disagree for per-endpoint policy), and a gate that both
reducers agree on single-entry configures.

Alternatively RFC-0065 D8's open question — one runtime per IMAGE on the cmake
road instead of one per coordinate — removes the shared runtime and with it
this issue; RFC-0100 Amendment 1 records that question as unmeasured.

## Acceptance

On `examples/workspaces/cpp`'s native configure, each derived count delivered
to the shared runtime equals the max over the entries' own derivations
(measured per entry), every entry still boots and delivers against
`rmw_zenohd`, and a gate fails when the shared value exceeds that max on a
fixture configure whose entries differ.

## Resolution (2026-10-06)

### Measured first: each entry's own derivation

Each of the seven models was derived ALONE (`nros ws entity-inventory --model
<one> --workspace`) over the native configure's own `nros-metadata.json`, on a
freshly synced workspace. That is what a single-entry configure of that entry
would get. The table below compares those against the union the shared runtime
was sized from:

| entry (model) | MAX_CBS | subs | pubs | queryables | TL pubs | liveliness | cell | local queryable |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| native (system) | 2 | 1 | 1 | 1 | 1 | 10 | 1 | 0 |
| native_action_client | 6 | 2 | 0 | 0 | 0 | 15 | 3 | 0 |
| native_action_server | 5 | 0 | 4 | 9 | 3 | 20 | 3 | 0 |
| native_service_client | 2 | 0 | 0 | 0 | 0 | 8 | 1 | 0 |
| native_service_server | 1 | 0 | 0 | 1 | 0 | 8 | 1 | 0 |
| native_robot1 / robot2 (multihost) | REFUSED alone | | | | | | | |
| **union (before)** | **16** | 3 | 5 | 11 | 4 | 33 | 3 | **1** |
| **max over entries (after)** | **6** | 2 | 4 | 9 | 3 | 20 | 3 | **0** |

The issue's "9 vs 2" was an older tree. Since then the union has grown to 16,
and the largest entry is the action client at 6, not the 2 of `native_entry`.
`EXECUTOR_MAX_NODES` (6) and `EXECUTOR_MAX_SC` (7) are equal across entries,
because both count every registered component, launched or not. That is a
separate over-count and is untouched here. `RMW_LOCAL_QUERYABLE` was on only
because one entry has a service client and a DIFFERENT entry has the server. No
single process queries its own session.

`native_robot1`/`2` run `multihost.launch.xml`, which has no contract. Derived
alone, each refuses: the components it starts (talker, listener) are described
only by `system.contract.yaml`.

### What changed

There is one reducer for both outputs: `DerivedEntityKnobs::max_over`, called
by `EntityInventory::derive` whenever the inventory carries per-image views. The
entity fragment and RFC-0100 D12's runtime descriptor both derive through it.
The per-field rules:

- **Counts:** the max, because every count is a capacity.
- **`local_queryable`:** an OR over images, never
  `max(clients) && max(servers)`.
- **Monitor tables:** unknown if any image's table is unknown.
- **Transient-local `Fact`:** the first refusal wins (D6).
- **`per_kind`:** the max per kind.
- **`per_component`:** the union of rows.

`shared_runtime_over_framed` builds each image's view. The view is the
single-image composition (metadata folded with that image's own model). A
component the image starts that its own model leaves undescribed takes the
statement another image's contract made of it. That is the existing rule that
a `Stated` row is a property of the component, not of the launch file. An
image whose model has no wiring still has an `ImageFrame` (its launch tree's
node set, families, tiers and monitor tables, factored out of `from_model` so
there is one spelling). The frame is what lets the robots be sized by what
they start instead of by the union. With no frame, an image is sized as the
union, so the reducer never goes below the union for an image it cannot see.

**Kept as the union, and why:**

- The subscribed/received/request type lists. The pool they size is one table
  for every type the runtime receives.
- The declared-QoS rows. These are per endpoint, under D12 rule 2's
  refuse-on-disagree.
- The parameter declarations. The same list renders the per-component
  declared-params table, which must hold every node of every image. A
  per-image max is not representable there.

### Acceptance

- **Delivered counts.** The native configure's fragment now states
  `NROS_DERIVED_EXECUTOR_MAX_CBS 6`, `MAX_QUERYABLES 9` and
  `RMW_LOCAL_QUERYABLE 0`, each equal to the max over the entries' derivations
  in the table above. The runtime descriptor states `subscriber_count = 2` and
  `publisher_count = 4`, down from 3 and 5.
- **Every entry boots and delivers against `rmw_zenohd`.**
  - `native_entry` was run by hand: `Published: N` and `Received: N` for 0..4.
  - `multihost_e2e` native/cpp (robot1 and robot2) passed.
  - `roundtrip_xprocess_e2e` cpp/service and cpp/action (all four of the
    service/action entries) passed.
  - `workspace_features_e2e` `case_12_cpp_logging` passed.
  - The other cells in those aggregate tests are other languages' fixtures,
    which were not built here.
- **The gate.** The gate is `a_shared_runtime_is_sized_for_its_largest_image_not_for_the_union`.
  - It runs on a fixture configure whose entries differ.
  - It asserts that each shared count EQUALS the max over the entries' own
    derivations, and that the union exceeds it.
  - Mutation check: forcing `derive` back to the union fails that test and the
    unwired-robot test.
  - It is joined by `with_one_image_the_reduction_is_the_union` (both reducers
    agree on a single-entry configure) and by tests for the unwired-image,
    refusal and per-field rules.
- **RFC-0100.** D12 carries a dated 2026-10-06 amendment note.
