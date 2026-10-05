---
id: 1701
title: "A multi-entry configure sizes its shared runtime for the UNION of its entries, not the largest one — `MAX_CBS` 9 where one entry needs 2"
status: open
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
