---
id: 2041
title: "A level image's executor arena is provisioned at 2.5x its measured use, and larger than the arena the image compiled"
status: resolved
type: enhancement
area: [executor, codegen, memory]
severity: low
found: 2026-10-25
resolved: 2026-10-26
resolved_in: "PR #2678 (nros-node arena model, nros-build-helpers size probe)"
related: [phase-509, rfc-0108, 2019, 2040]
---

## Measured

simple-autoware-safety-island board image (Zephyr, S32K344, three levels on
one executor, nano-ros 4f545011ff). The boot record read over SWD after the
`b` act (run p2618-pb, 16/16 PASS):

```
compiled in:
  NROS_EXECUTOR_ARENA_SIZE      25724
measured on the board:
  arena capacity                29820
  arena used                    11976   (40.2%)
  arena allocations             12
NOTE: the executor was handed 29820 bytes but the image compiled 25724.
```

The arena lives inside `__nros_levels_backing`
(`NROS_CPP_LEVELS_BACKING_SIZE` = 40,640 B, every level's members summed), and
on this board that static sits in DTCM, the scarcer region (83.66 % used).

Two things look wrong:

1. The arena is sized at 2.5x what the image uses: 17,844 B of DTCM are never
   claimed. phase-403 W3/W5 claim the arena per type at registration, but the
   island's board conf already records that "the derivation still provisions
   the old way" (max_cbs x worst case).
2. The level backing hands the executor 29,820 B while the image compiled
   `NROS_EXECUTOR_ARENA_SIZE` 25,724. The boot record flags it as legitimate
   (issue 0900: placement is the caller's), but the two derivations disagree,
   and the larger one is the one paid for.

## Closes when

- The levels backing sizes its arena from the per-type claims the members
  register (the same bound the first-spin advisory prints), not from
  max_cbs x worst case, and agrees with `NROS_EXECUTOR_ARENA_SIZE`.
- A board or fixture boot record shows arena use within a stated margin of
  capacity on a level image.

## Resolution

Three defects, each measured on the island image (nano-ros main f75607307f).

1. **The base was paid once per level.** Issue 1970 prices each tier as its
   own executor, so each tier's arena carries `arena_model::BASE_OVERHEAD`
   (2,048 B). The level runner opens ONE executor, but
   `levels_executor_sizing` summed the tiers: 6,128 + 7,500 + 16,192 =
   29,820, which is 25,724 (the image's model) plus two bases. It now pays
   the base once (`levels_arena_bytes`), so the level arena is the image's
   model, the value `NROS_EXECUTOR_ARENA_SIZE` derives to.
2. **The `in_place` subscriptions were priced at a receive region.** The
   sizing descriptor states the per-type rows (`registration_path =
   in_place`, depth 1 for all seven subscriptions). `descriptor_subscriptions`
   refused the whole table because `[meta] undeclared_endpoints` was 4. Those
   four are the service endpoints, which carry no QoS fact. The rows price
   subscriptions only, and a subscription that declared nothing has no
   depth, which `subs_arena_from_descriptor` already refuses row by row. The
   guard now requires only that the count is KNOWN. With the rows, an
   `in_place` subscription costs its entry struct and not 3 x its type bound
   (3,372 B over the seven).
3. **The size probe disagreed with the linked executor.** The backing a C++
   level entry declares (`NROS_CPP_LEVELS_BACKING_SIZE`) comes from a nested
   probe build of `nros`. That build does not link the backend, so its
   `nros-rmw` lacked `in-place-dispatch` and its `nros-node` kept the
   receive regions (backing 36,544 B while the linked executor needed
   33,176). The probe now builds `nros-rmw` with the dispatch features the
   outer graph reports (`DEP_NROS_RMW_*`).

The arena is still a MODEL, not a measurement: it budgets each kind's
entry allowance (`PUBSUB_STRUCT`, `SERVICE_ENTRY`, ...). It also budgets the
two service clients, which the island creates poll-style and which claim no
arena. The descriptor cannot tell a poll client from a callback one.

## Evidence

Board, S32K344, island 1bfab8c on nano-ros f75607307f plus this change and
the cmake dependency fix (PR #2676). Boot record, every act:

```
  NROS_EXECUTOR_ARENA_SIZE      22352
  arena capacity                22352
  arena used                    11992   (53.7%)
  arena allocations             12
```

No `NOTE` line: capacity equals the compiled size. **Margin: use is over
half of capacity.** That is the first-spin advisory's own rule (issue 0900,
`arena_is_over_provisioned`), so the advisory no longer fires. Before the
change: 11,976 of 29,820, 40.2 %. The other 10,360 B are named terms: the two
poll clients (4,912), one base (2,048), and the entry allowances above
`size_of` on thumbv7em (measured: an in-place C subscription entry is 660 B
against 1,024; a raw service entry is 3,596 against 4,016).

| | before | after |
| --- | --- | --- |
| level arena | 29,820 B | 22,352 B |
| `__nros_levels_backing` | 40,640 B | 33,176 B |
| DTCM used | 109,660 B | 102,196 B |
| RAM used | 269,392 B | 269,392 B |

The planning acts encore, a, b and the overrun encore (`RB_OVERRUN=300`) all
PASS. SWT0 fires 0, and the priorities agree with the plan.
